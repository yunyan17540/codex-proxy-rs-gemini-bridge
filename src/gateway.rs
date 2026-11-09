//! 通过宿主公开管理入口幂等创建本实例 Gemini 上游与独立分组

use crate::engine::Bridge;
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::{
        middleware::{MiddlewareHeader, http::Version},
        resources::{GroupEnsureRequest, GroupMembersChange},
    },
    client::{HostClient, HttpBody, HttpFrame, HttpRequest},
};
use serde_json::{Value, json};

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
            "网关操作失败（HTTP {}），请查看网关运维记录",
            response.status
        )));
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| fault("宿主管理响应缺少数据"))
}

pub async fn connect(
    host: &HostClient,
    bridge: &Bridge,
    models: &[String],
) -> Result<Value, PluginFault> {
    if models.is_empty() || models.len() > 256 {
        return Err(fault("请先完成 Google 授权并刷新 Gemini 模型"));
    }
    let name = format!("Gemini 本机中转 {}", bridge.instance_id);
    let marker = format!("local.gemini-bridge/{}", bridge.instance_id);
    let group = host
        .ensure_group(GroupEnsureRequest {
            resource_key: "gemini".into(),
            // 宿主删除实例会保留原分组；新归属首次建组时使用新名称，避免重装冲突。
            name: format!("Gemini {}", uuid::Uuid::new_v4().simple()),
            color: "#2563EBFF".into(),
            description: Some(format!(
                "本机 Gemini CLI 谷歌账号，实例 {}",
                bridge.instance_id
            )),
        })
        .await?;
    // 按实例标记扫描完整目录；账号改名或被移出分组后仍复用原记录。
    let mut matching = Vec::new();
    for page in 1..=64 {
        let accounts = admin(
            host,
            "GET",
            &format!("/api/admin/accounts?provider=openai&pageSize=100&page={page}"),
            None,
        )
        .await?;
        let items = accounts
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| fault("网关账号目录格式无效"))?;
        matching.extend(
            items
                .iter()
                .filter(|account| account.get("notes").and_then(Value::as_str) == Some(&marker))
                .cloned(),
        );
        if items.len() < 100 {
            break;
        }
        if page == 64 {
            return Err(fault("网关账号目录过大，未执行导入"));
        }
    }
    if matching.len() > 1 {
        return Err(fault("发现多个本插件上游，请在账号页面保留一个后再连接"));
    }
    let account_id = if let Some(account) = matching.first() {
        let account_id = account
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| fault("Gemini 上游缺少账号标识"))?;
        let mut groups: Vec<Value> = account
            .get("groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|group| group.get("id").cloned())
            .collect();
        if !groups.iter().any(|id| id.as_str() == Some(&group.id)) {
            groups.push(json!(group.id));
        }
        admin(host, "POST", "/api/admin/accounts/update", Some(json!({
            "accountId":account_id, "enabled":account.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            "concurrencyLimit":account.get("concurrencyLimit").cloned().unwrap_or(Value::Null),
            "weight":account.get("weight").and_then(Value::as_u64).unwrap_or(50), "groupIds":groups,
            "modelAccess":{"mode":"allowlist","models":models},
            "connection":{"baseUrl":format!("{}/v1",bridge.base_url()),"apiKey":bridge.client_key()?,"transport":"http"}
        }))).await?;
        account_id.to_owned()
    } else {
        let result = admin(host, "POST", "/api/admin/accounts/import", Some(json!({
            "provider":"openai", "data":{"authentication_kind":"api_key","name":name,"base_url":format!("{}/v1",bridge.base_url()),"api_key":bridge.client_key()?,"transport":"http"},
            "settings":{"enabled":true,"concurrencyLimit":2,"weight":50,"groupIds":[group.id],"notes":marker,"modelAccess":{"mode":"allowlist","models":models}}
        }))).await?;
        // 从实际创建结果取标识，不在结果未知时重复提交导入
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
                    .and_then(|ids| ids.first())
                    .and_then(Value::as_str)
            })
            .map(str::to_owned)
            .ok_or_else(|| fault("Gemini 上游已提交，请刷新账号列表核对结果后再连接"))?
    };
    host.change_group_members(GroupMembersChange {
        resource_key: "gemini".into(),
        add: vec![account_id.clone()],
        remove: vec![],
    })
    .await?;
    Ok(
        json!({"accountId":account_id,"groupId":group.id,"groupName":group.name,"models":models,"message":"已连接 Gemini 上游，受分组限制的使用密钥需要加入此分组"}),
    )
}
