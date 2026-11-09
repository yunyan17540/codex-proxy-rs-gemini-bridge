//! 只返回静态诊断文案；引擎原始错误可能带令牌、回调或用户资料，不透传。
use crate::gateway::fault;
use gateway_plugin_sdk::PluginFault;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum LoginProvider {
    #[default]
    #[serde(rename = "antigravity")]
    Antigravity,
    #[serde(rename = "gemini-cli")]
    GeminiCli,
}

impl LoginProvider {
    pub fn key(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::GeminiCli => "gemini-cli",
        }
    }
}

pub fn error_message(raw: &str) -> &'static str {
    let error = raw.to_ascii_lowercase();
    if error.contains("invalid_grant") {
        "Google 授权码已过期或已使用。请取消后生成新链接，完成授权后不要重复提交旧回调。"
    } else if error.contains("redirect_uri_mismatch") {
        "Google 回调地址不匹配。请重新生成授权链接，保留本机中转端口。"
    } else if error.contains("invalid_client") || error.contains("unauthorized_client") {
        "Google 拒绝了当前登录客户端，请检查 Gemini CLI 模块的版本与授权资格。"
    } else if error.contains("access_denied") {
        "Google 授权被拒绝或取消，请重新打开授权链接并核对申请的权限。"
    } else if error.contains("unsupported_country")
        || error.contains("unsupported_location")
        || error.contains("unknown_location")
        || error.contains("not available in your country")
        || error.contains("location is not supported")
    {
        "Google 报告当前地区不受支持，请核对 Gemini 专用代理的实际出口。"
    } else if error.contains("project_id is required") || error.contains("without project id") {
        "Google 未能分配 Code Assist 项目，请填写有权限的 Google Cloud 项目 ID 后重试。"
    } else if error.contains("project list request failed: status 403") {
        "Google 项目列表权限不足。请填写有权限的 Google Cloud 项目 ID，并检查 Code Assist 资格。"
    } else if error.contains("subscription_required") {
        "Google 要求有效的 Code Assist Standard/Enterprise 订阅。个人账号请使用 Antigravity 通道，新建 Cloud 项目不会获得此订阅。"
    } else if error.contains("service_disabled") || error.contains("accessnotconfigured") {
        "Google 报告当前项目未启用所需服务，请在有权限的项目中检查 Code Assist 服务设置。"
    } else if error.contains("validation_required")
        || error.contains("account validation is required")
    {
        "Google 要求验证账号。请先在官方 Gemini CLI 完成提示的账号验证，再回来授权。"
    } else if error.contains("restricted_network") {
        "Google 判定当前网络不符合个人 Code Assist 使用条件，请更换可用出口后再试。"
    } else if error.contains("restricted_age") {
        "Google 报告账号年龄条件不符合 Code Assist 要求，请在 Google 账号中检查年龄及验证状态。"
    } else if error.contains("ineligible_account")
        || error.contains("non_user_account")
        || error.contains("dasher_user")
    {
        "Google 报告该账号不符合个人 Code Assist 条件，请在官方 Gemini CLI 查看账号资格要求。"
    } else if error.contains("tos_violation") {
        "Google 返回 TOS_VIOLATION，请在官方 Gemini CLI 查看该账号的服务状态及申诉入口。"
    } else if error.contains("status 403") || error.contains("permission_denied") {
        "Google 返回 403 权限拒绝，请检查账号的 Gemini CLI / Code Assist 资格、项目权限和地区。"
    } else if error.contains("timed out")
        || error.contains("timeout")
        || error.contains("deadline exceeded")
    {
        "Google 授权或初始化超时。请检查出站代理，生成新链接后在三分钟内完成登录。"
    } else if error.contains("proxyconnect")
        || error.contains("connection refused")
        || error.contains("tls handshake")
        || error.contains("eof")
        || error.contains("no such host")
    {
        "连接 Google 失败，请检查出站代理以及 Google 令牌、Code Assist 接口是否走同一可用节点。"
    } else if error.contains("load code assist") || error.contains("onboard user") {
        "Google 登录后的 Code Assist 初始化失败，请核对账号资格、项目 ID 和出站代理。"
    } else if error.contains("unknown or expired state") {
        "这次 Google 授权已失效，请生成新链接后重试。"
    } else {
        "Google 授权未完成。请检查账号的 Code Assist 资格、项目权限和出站代理后生成新链接。"
    }
}

/// 只保留预定义阶段、状态码和原因，不能拼接 Google 的原始描述或用户资料。
pub fn diagnostic_message(raw: &str) -> String {
    let error = raw.to_ascii_lowercase();
    let phase = if error.contains("poll onboarding operation") {
        "查询初始化进度"
    } else if error.contains("load code assist") {
        "读取 Code Assist 资格"
    } else if error.contains("onboard user") || error.contains("autodiscovery onboarduser") {
        "初始化 Code Assist"
    } else if error.contains("token request failed") {
        "交换 Google 令牌"
    } else if error.contains("project list request failed") {
        "查询 Cloud 项目"
    } else {
        "Google 授权"
    };
    let status = [400, 401, 403, 404, 409, 429, 500, 502, 503, 504]
        .into_iter()
        .find(|code| {
            error.contains(&format!("status {code}:")) || error.ends_with(&format!("status {code}"))
        });
    let reason = [
        "SUBSCRIPTION_REQUIRED",
        "SERVICE_DISABLED",
        "VALIDATION_REQUIRED",
        "RESTRICTED_NETWORK",
        "RESTRICTED_AGE",
        "UNSUPPORTED_LOCATION",
        "UNKNOWN_LOCATION",
        "INELIGIBLE_ACCOUNT",
        "NON_USER_ACCOUNT",
        "DASHER_USER",
        "TOS_VIOLATION",
        "IAM_PERMISSION_DENIED",
        "USER_PROJECT_DENIED",
        "INSUFFICIENT_AUTHENTICATION_SCOPES",
        "PERMISSION_DENIED",
    ]
    .into_iter()
    .find(|reason| error.contains(&reason.to_ascii_lowercase()));
    let mut context = phase.to_owned();
    if let Some(status) = status {
        context.push_str(&format!(" · HTTP {status}"));
    }
    if let Some(reason) = reason {
        context.push_str(" · ");
        context.push_str(reason);
    }
    format!("{context}：{}", error_message(raw))
}

/// 按本次官方授权链接的 redirect_uri 校验，不假设固定的旧版回调路径。
pub fn validate_callback(expected: &str, state: &str, input: &str) -> Result<(), PluginFault> {
    let expected = url::Url::parse(expected).map_err(|_| fault("当前授权缺少有效回调地址"))?;
    let received = url::Url::parse(input.trim()).map_err(|_| fault("请输入完整本机回调地址"))?;
    if input.len() > 16 * 1024
        || received.scheme() != "http"
        || ![Some("localhost"), Some("127.0.0.1")].contains(&received.host_str())
        || !received.username().is_empty()
        || received.password().is_some()
        || received.fragment().is_some()
        || received.port_or_known_default() != expected.port_or_known_default()
        || received.path() != expected.path()
    {
        return Err(fault("回调地址与本次授权的本机端口或路径不匹配"));
    }
    let states: Vec<_> = received
        .query_pairs()
        .filter(|(key, _)| key == "state")
        .collect();
    if states.len() != 1 || states[0].1 != state {
        return Err(fault("回调与当前 Google 授权不匹配"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn google_channels_are_limited_and_default_to_antigravity() {
        assert_eq!(LoginProvider::default(), LoginProvider::Antigravity);
        assert_eq!(LoginProvider::GeminiCli.key(), "gemini-cli");
        assert_eq!(
            serde_json::from_str::<LoginProvider>("\"antigravity\"").expect("supported provider"),
            LoginProvider::Antigravity
        );
        assert!(serde_json::from_str::<LoginProvider>("\"other-provider\"").is_err());
    }
    #[test]
    fn accepts_the_actual_v8_callback_and_rejects_other_targets() {
        let expected = "http://127.0.0.1:18764/v8/management/oauth/callback";
        assert!(
            validate_callback(
                expected,
                "test-state",
                &format!("{expected}?state=test-state&code=fake-code")
            )
            .is_ok()
        );
        for received in [
            "http://127.0.0.1:18764/oauth2callback?state=test-state",
            "http://127.0.0.1:18765/v8/management/oauth/callback?state=test-state",
            "https://example.com/v8/management/oauth/callback?state=test-state",
            "http://127.0.0.1:18764/v8/management/oauth/callback?state=wrong",
            "http://127.0.0.1:18764/v8/management/oauth/callback?state=test-state&state=wrong",
        ] {
            assert!(validate_callback(expected, "test-state", received).is_err());
        }
        assert!(
            validate_callback(
                "http://localhost:51121/oauth-callback",
                "test-state",
                "http://127.0.0.1:51121/oauth-callback?state=test-state&code=fake-code",
            )
            .is_ok()
        );
    }
    #[test]
    fn diagnostics_never_echo_raw_error_or_login_material() {
        let raw = "gemini-cli token request failed: status 400: invalid_grant access_token=PRIVATE";
        assert!(error_message(raw).contains("授权码"));
        assert!(!error_message(raw).contains("PRIVATE"));
        assert!(!error_message("unknown PRIVATE user@example.test").contains("PRIVATE"));
        let detail = diagnostic_message(
            "load code assist: api request failed with status 403: {\"reason\":\"SUBSCRIPTION_REQUIRED\",\"email\":\"PRIVATE@example.test\"}",
        );
        assert!(detail.contains("读取 Code Assist 资格"));
        assert!(detail.contains("HTTP 403"));
        assert!(detail.contains("SUBSCRIPTION_REQUIRED"));
        assert!(!detail.contains("PRIVATE"));
        assert!(!detail.contains("example.test"));
        assert!(!diagnostic_message("unrecognized PRIVATE_URL").contains("PRIVATE"));
    }
}
