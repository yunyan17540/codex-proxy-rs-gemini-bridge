//! 每个 Google 身份对应一条宿主连接；旧合并连接只停用，保留历史。
use crate::{
    accounts::{self, Account},
    engine::Bridge,
};
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::{
        middleware::{MiddlewareHeader, http::Version},
        resources::{GroupEnsureRequest, GroupMembersChange},
    },
    client::{HostClient, HttpBody, HttpFrame, HttpRequest},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, time::Duration};

pub fn fault(message: &str) -> PluginFault {
    PluginFault::new(ErrorCode::Rejected, message)
}

pub async fn admin(
    host: &HostClient,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, PluginFault> {
    let (headers, body) = match body {
        Some(body) => (
            vec![MiddlewareHeader {
                name: "content-type".into(),
                value: b"application/json".to_vec(),
            }],
            HttpBody::from_bytes(serde_json::to_vec(&body).map_err(|_| fault("宿主请求编码失败"))?),
        ),
        None => (vec![], HttpBody::empty()),
    };
    let mut response = host
        .dispatch_http(HttpRequest {
            settings: Value::Null,
            method: method.into(),
            uri: path.into(),
            version: Version::Http11,
            headers,
            timeout_ms: Some(15000),
            body,
        })
        .await?;
    let mut bytes = Vec::new();
    while let Some(frame) = response.body.read().await? {
        if let HttpFrame::Data(chunk) = frame {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                response.body.close().await?;
                return Err(fault("宿主管理响应过大"));
            }
            bytes.extend_from_slice(&chunk);
        }
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| fault("宿主管理响应格式无效"))?;
    if !(200..300).contains(&response.status)
        || value.get("code").and_then(Value::as_u64) != Some(200)
    {
        return Err(fault(&format!(
            "网关操作失败（HTTP {}），请刷新后重试",
            response.status
        )));
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| fault("宿主管理响应缺少数据"))
}

pub fn marker(instance: &str, id: &str) -> String {
    format!("local.gemini-bridge/{instance}/{id}")
}

pub async fn catalog(host: &HostClient) -> Result<Vec<Value>, PluginFault> {
    let mut result = vec![];
    for page in 1..=64 {
        let value = admin(
            host,
            "GET",
            &format!("/api/admin/accounts?provider=openai&pageSize=100&page={page}"),
            None,
        )
        .await?;
        let items = value
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| fault("网关账号目录格式无效"))?;
        result.extend(items.iter().cloned());
        if items.len() < 100 {
            return Ok(result);
        }
    }
    Err(fault("网关账号目录过大，未执行账号变更"))
}

fn owned<'a>(catalog: &'a [Value], instance: &str, id: &str) -> Vec<&'a Value> {
    let marker = marker(instance, id);
    catalog
        .iter()
        .filter(|a| a.get("notes").and_then(Value::as_str) == Some(marker.as_str()))
        .collect()
}

fn settings(account: &Value, enabled: bool) -> Result<Value, PluginFault> {
    let id = account
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| fault("网关连接缺少账号标识"))?;
    Ok(json!({"accountId":id,"enabled":enabled,
        "concurrencyLimit":account.get("concurrencyLimit").cloned().unwrap_or(Value::Null),
        "weight":account.get("weight").and_then(Value::as_u64).unwrap_or(50),
        "groupIds":account.get("groups").and_then(Value::as_array).into_iter().flatten().filter_map(|g|g.get("id").cloned()).collect::<Vec<_>>() }))
}

pub async fn connect(
    host: &HostClient,
    bridge: &mut Bridge,
    selected: Option<&str>,
) -> Result<Value, PluginFault> {
    let mut google = accounts::load(bridge).await?;
    if let Some(id) = selected {
        if !accounts::valid_id(id) || !google.iter().any(|a| a.id == id) {
            return Err(fault("此 Google 账号已不存在，请刷新列表"));
        }
        google.retain(|a| a.id == id);
    }
    if google.is_empty() {
        return Err(fault("请先添加 Google 账号"));
    }
    let existing = catalog(host).await?;
    for account in &google {
        if owned(&existing, &bridge.instance_id, &account.id).len() > 1 {
            return Err(fault("此 Google 账号存在重复网关连接，请先清理重复项"));
        }
    }
    for account in &google {
        accounts::isolate(bridge, account).await?;
    }
    // 模型注册由本机文件观察器发布；只等本机目录，不进行推理或额度探测。
    let ids: Vec<_> = google.iter().map(|a| a.id.clone()).collect();
    for _ in 0..10 {
        let current = accounts::load(bridge).await?;
        google = current
            .into_iter()
            .filter(|a| ids.contains(&a.id))
            .collect();
        if google
            .iter()
            .all(|a| !a.models.is_empty() || a.disabled || a.requires_reauth)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if google.len() != ids.len() {
        return Err(fault("同步期间账号目录发生变化，请刷新后重试"));
    }
    let group = host
        .ensure_group(GroupEnsureRequest {
            resource_key: "gemini".into(),
            name: format!("Gemini {}", uuid::Uuid::new_v4().simple()),
            color: "#2563EBFF".into(),
            description: Some("每条连接固定对应一个 Google 账号".into()),
        })
        .await?;
    let paused_path = bridge.directory.join("gateway-paused-accounts.json");
    let mut paused: BTreeSet<String> = if paused_path.exists() {
        serde_json::from_slice(&fs::read(&paused_path).map_err(|_| fault("无法读取账号恢复状态"))?)
            .map_err(|_| fault("账号恢复状态损坏"))?
    } else {
        BTreeSet::new()
    };
    let mut results = vec![];
    let mut added = vec![];
    for account in &google {
        let matching = owned(&existing, &bridge.instance_id, &account.id);
        let previous = matching.first().copied();
        let blocked = account.disabled || account.requires_reauth || account.models.is_empty();
        let was_enabled = previous
            .and_then(|a| a.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if blocked && was_enabled && !account.disabled {
            paused.insert(account.id.clone());
        }
        let recovering = !blocked && paused.remove(&account.id);
        let enabled = !blocked && (was_enabled || recovering);
        let key = bridge.account_key(&account.id, !blocked)?;
        let connection = json!({"baseUrl":bridge.account_base_url(&account.id)?,"apiKey":key,"transport":"http"});
        let models = if account.models.is_empty() {
            previous
                .and_then(|a| a.get("modelAccess"))
                .and_then(|a| a.get("models"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        } else {
            account.models.iter().map(|m| json!(m)).collect()
        };
        let model_access = if models.is_empty() {
            json!({"mode":"all","models":[]})
        } else {
            json!({"mode":"allowlist","models":models})
        };
        let account_id = if let Some(previous) = previous {
            let mut update = settings(previous, enabled)?;
            let groups = update["groupIds"].as_array_mut().expect("groups array");
            if !groups.iter().any(|g| g.as_str() == Some(&group.id)) {
                groups.push(json!(group.id));
            }
            update["connection"] = connection;
            update["modelAccess"] = model_access.clone();
            admin(host, "POST", "/api/admin/accounts/update", Some(update)).await?;
            previous["id"]
                .as_str()
                .expect("checked account id")
                .to_owned()
        } else {
            let result=admin(host,"POST","/api/admin/accounts/import",Some(json!({"provider":"openai",
                "data":{"authentication_kind":"api_key","name":format!("Gemini · {}",account.email.as_deref().unwrap_or(&account.id)),"base_url":bridge.account_base_url(&account.id)?,"api_key":key,"transport":"http"},
                "settings":{"enabled":enabled,"concurrencyLimit":2,"weight":50,"groupIds":[group.id],"notes":marker(&bridge.instance_id,&account.id),"modelAccess":model_access}
            }))).await?;
            result
                .get("accounts")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|a| a.get("id").or_else(|| a.get("accountId")))
                .and_then(Value::as_str)
                .or_else(|| {
                    result
                        .get("accountIds")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(Value::as_str)
                })
                .map(str::to_owned)
                .ok_or_else(|| fault("账号连接已提交，请刷新后核对，不要重复导入"))?
        };
        added.push(account_id.clone());
        results.push(json!({"id":account.id,"accountId":account_id,"enabled":enabled,"modelCount":account.models.len()}));
    }
    fs::write(
        &paused_path,
        serde_json::to_vec(&paused).map_err(|_| fault("账号恢复状态编码失败"))?,
    )
    .map_err(|_| fault("无法保存账号恢复状态"))?;
    host.change_group_members(GroupMembersChange {
        resource_key: "gemini".into(),
        add: added,
        remove: vec![],
    })
    .await?;
    if selected.is_none() {
        let legacy = format!("local.gemini-bridge/{}", bridge.instance_id);
        for account in existing
            .iter()
            .filter(|a| a.get("notes").and_then(Value::as_str) == Some(legacy.as_str()))
        {
            admin(
                host,
                "POST",
                "/api/admin/accounts/update",
                Some(settings(account, false)?),
            )
            .await?;
        }
    }
    Ok(
        json!({"accounts":results,"groupId":group.id,"groupName":group.name,"message":"已按 Google 账号建立独立连接；旧合并上游已停用，历史记录保留"}),
    )
}

pub async fn delete(
    host: &HostClient,
    bridge: &mut Bridge,
    id: &str,
) -> Result<Value, PluginFault> {
    if !accounts::valid_id(id) {
        return Err(fault("Google 账号标识无效"));
    }
    if bridge.oauth_account.as_deref() == Some(id) {
        return Err(fault("请先完成或取消此账号等待中的授权"));
    }
    let accounts = accounts::load(bridge).await?;
    let target = accounts.iter().find(|a| a.id == id);
    let existing = catalog(host).await?;
    let connected = owned(&existing, &bridge.instance_id, id);
    if target.is_none() && connected.is_empty() {
        return Err(fault("此账号已经删除，请刷新列表"));
    }
    // 先关闭连接；即使清理中途失败，也不能把流量落到其他 Google 账号。
    for account in &connected {
        admin(
            host,
            "POST",
            "/api/admin/accounts/update",
            Some(settings(account, false)?),
        )
        .await?;
    }
    bridge.remove_account_route(id)?;
    if let Some(account) = target {
        bridge
            .call(
                reqwest::Method::PATCH,
                "/v8/management/credentials/status",
                Some(json!({"name":account.name,"auth_index":id,"disabled":true})),
                false,
            )
            .await?;
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("name", &account.name)
            .finish();
        bridge
            .call(
                reqwest::Method::DELETE,
                &format!("/v8/management/credentials?{query}"),
                None,
                false,
            )
            .await?;
    }
    let ids: Vec<_> = connected
        .iter()
        .filter_map(|a| a.get("id").and_then(Value::as_str))
        .collect();
    if !ids.is_empty() {
        admin(
            host,
            "POST",
            "/api/admin/accounts/delete",
            Some(json!({"provider":"openai","accountIds":ids})),
        )
        .await?;
    }
    Ok(json!({"message":"已删除该账号的本机授权及对应网关连接；Google 账号本身不会删除"}))
}

pub fn connection_id<'a>(
    catalog: &'a [Value],
    instance: &str,
    account: &Account,
) -> Option<&'a str> {
    owned(catalog, instance, &account.id)
        .first()
        .and_then(|a| a.get("id"))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_never_matches_another_instance_account_or_old_pool() {
        let records = vec![
            json!({"id":"one","notes":marker("instance","one")}),
            json!({"id":"two","notes":marker("instance","two")}),
            json!({"id":"legacy","notes":"local.gemini-bridge/instance"}),
            json!({"id":"other","notes":marker("different","one")}),
        ];
        assert_eq!(owned(&records, "instance", "one").len(), 1);
        assert_eq!(owned(&records, "instance", "one")[0]["id"], "one");
    }
}
