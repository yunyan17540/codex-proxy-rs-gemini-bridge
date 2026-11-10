//! Google 凭据的独立身份、状态投影及本机定向模型目录。
use crate::{engine::Bridge, gateway::fault};
use gateway_plugin_sdk::PluginFault;
use reqwest::Method;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub email: Option<String>,
    pub provider: String,
    pub status: String,
    pub message: String,
    pub disabled: bool,
    pub unavailable: bool,
    pub requires_reauth: bool,
    pub next_retry_after: Option<String>,
    pub models: Vec<String>,
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub prefix: String,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

pub fn prefix(id: &str) -> String {
    format!("gb-{id}")
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn from_entry(v: &Value) -> Option<Account> {
    let provider = v.get("provider").or_else(|| v.get("type"))?.as_str()?;
    if !["antigravity", "gemini-cli", "gemini"].contains(&provider) {
        return None;
    }
    let id = text(v, "auth_index");
    let name = text(v, "name");
    if !valid_id(id) || name.is_empty() {
        return None;
    }
    let disabled = v.get("disabled").and_then(Value::as_bool).unwrap_or(false);
    let unavailable = v
        .get("unavailable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Access token 到期本身不是登录失效；额度/冷却也不能标成需要重登。
    let error = format!(
        "{} {} {} {} {} {} {}",
        text(v, "status_message"),
        text(v, "renewal_state"),
        text(v, "renewal_reason"),
        text(v, "renewal_reason_code"),
        v.get("last_error").unwrap_or(&Value::Null),
        text(v, "recovery_stage"),
        v.get("refresh_error").unwrap_or(&Value::Null)
    )
    .to_lowercase();
    let requires_reauth = v.get("requires_sign_in").and_then(Value::as_bool) == Some(true)
        || [
            "invalid_grant",
            "invalid_token",
            "reauth_required",
            "reauthentication_required",
            "refresh_token_expired",
            "token_revoked",
            "unauthenticated",
            "credential_invalidated",
            "login_required",
        ]
        .iter()
        .any(|s| error.contains(s));
    let quota = v.get("quota_blocked").and_then(Value::as_bool) == Some(true)
        || error.contains("quota")
        || error.contains("resource_exhausted")
        || error.contains("rate_limit")
        || v.get("quota")
            .and_then(|q| q.get("exceeded"))
            .and_then(Value::as_bool)
            == Some(true);
    let (status, message) = if disabled {
        ("disabled", "已停用")
    } else if requires_reauth {
        ("reauth_required", "授权已失效，请重新授权")
    } else if quota {
        (
            "quota_limited",
            "额度或速率受限，等待恢复；不需要因此重新登录",
        )
    } else if unavailable {
        (
            "temporarily_unavailable",
            "暂不可用，可查看账号授权或稍后刷新",
        )
    } else if text(v, "status") == "error" {
        ("error", "账号返回错误，可重新授权或稍后刷新")
    } else {
        ("active", "已授权")
    };
    Some(Account {
        id: id.into(),
        name: name.into(),
        provider: provider.into(),
        email: v.get("email").and_then(Value::as_str).map(str::to_owned),
        status: status.into(),
        message: message.into(),
        disabled,
        unavailable,
        requires_reauth,
        next_retry_after: v
            .get("next_retry_after")
            .and_then(Value::as_str)
            .map(str::to_owned),
        prefix: text(v, "prefix").into(),
        models: vec![],
    })
}

pub async fn load(bridge: &Bridge) -> Result<Vec<Account>, PluginFault> {
    let value = bridge
        .call(Method::GET, "/v8/management/credentials", None, false)
        .await?;
    let entries = value
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| fault("Google 账号目录格式无效"))?;
    let mut accounts: Vec<_> = entries.iter().filter_map(from_entry).collect();
    accounts.sort_by(|a, b| a.id.cmp(&b.id));
    if accounts.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(fault("Google 账号身份重复，已停止同步"));
    }
    for account in &mut accounts {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("name", &account.name)
            .finish();
        let value = bridge
            .call(
                Method::GET,
                &format!("/v8/management/credentials/models?{query}"),
                None,
                false,
            )
            .await?;
        account.models = value
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .map(|model| {
                model
                    .strip_prefix(&format!("{}/", prefix(&account.id)))
                    .unwrap_or(model)
            })
            .filter(|model| model.starts_with("gemini-") && model.len() <= 256)
            .map(str::to_owned)
            .collect();
        account.models.sort();
        account.models.dedup();
    }
    Ok(accounts)
}

pub async fn find(bridge: &Bridge, id: &str) -> Result<Account, PluginFault> {
    if !valid_id(id) {
        return Err(fault("Google 账号标识无效"));
    }
    load(bridge)
        .await?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| fault("此 Google 账号已不存在，请刷新列表"))
}

pub async fn isolate(bridge: &Bridge, account: &Account) -> Result<(), PluginFault> {
    let expected = prefix(&account.id);
    // 使用引擎自己的唯一模型前缀限制选号，而非克隆同一个共享池连接。
    if account.prefix != expected {
        bridge
            .call(
                Method::PATCH,
                "/v8/management/credentials/fields",
                Some(json!({"name":account.name,"prefix":expected})),
                false,
            )
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renewal_is_distinct_from_quota_and_access_expiry() {
        let sample = json!({"provider":"antigravity","name":"fixture.json","auth_index":"abc123","status":"active","expired":"2000-01-01T00:00:00Z"});
        assert_eq!(from_entry(&sample).unwrap().status, "active");
        let mut value = sample.clone();
        value["unavailable"] = json!(true);
        value["status_message"] = json!("RESOURCE_EXHAUSTED: quota");
        let account = from_entry(&value).unwrap();
        assert_eq!(account.status, "quota_limited");
        assert!(!account.requires_reauth);
        value["status_message"] = json!("invalid_grant: token revoked");
        assert!(from_entry(&value).unwrap().requires_reauth);
        let mut flags = sample;
        flags["requires_sign_in"] = json!(true);
        assert!(from_entry(&flags).unwrap().requires_reauth);
        flags["requires_sign_in"] = json!(false);
        flags["quota_blocked"] = json!(true);
        assert_eq!(from_entry(&flags).unwrap().status, "quota_limited");
        assert!(!valid_id("../other"));
        assert!(!valid_id("a?all=true"));
    }
    #[test]
    fn status_projection_never_serializes_credentials_or_internal_filename() {
        let value = json!({"provider":"antigravity","name":"private-name.json","auth_index":"abc123","access_token":"fixture-access","refresh_token":"fixture-refresh","status_message":"fixture-refresh"});
        let output = serde_json::to_string(&from_entry(&value).unwrap()).unwrap();
        for secret in ["fixture-access", "fixture-refresh", "private-name.json"] {
            assert!(!output.contains(secret));
        }
    }
}
