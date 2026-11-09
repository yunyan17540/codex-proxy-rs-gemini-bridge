//! 隔离管理页面的 Google 授权、账号状态与显式网关连接

use crate::{
    engine::{Bridge, gemini_models},
    gateway::{self, fault},
};
use gateway_plugin_sdk::{
    PluginFault,
    call::management::*,
    client::{TypedCall, TypedReply},
};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

pub fn registration() -> ManagementRegistration {
    let route = |method: &str, path: &str| ManagementRoute {
        method: method.into(),
        path: path.into(),
        request_content_types: if method == "GET" {
            vec![]
        } else {
            vec!["application/json".into()]
        },
        response_content_types: vec!["application/json".into()],
    };
    ManagementRegistration {
        routes: vec![
            route("GET", "api/status"),
            route("POST", "api/login"),
            route("POST", "api/open-login"),
            route("POST", "api/poll"),
            route("POST", "api/callback"),
            route("POST", "api/cancel"),
            route("POST", "api/connect"),
        ],
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|path| ManagementResource {
                path: path.into(),
                public: false,
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "gemini".into(),
            title: "Gemini 谷歌账号".into(),
            description: Some("Google 授权与 Gemini 模型连接".into()),
            entry: "web/index.html".into(),
            icon: None,
        }],
        callbacks: vec![],
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    #[serde(default)]
    provider: crate::oauth::LoginProvider,
    #[serde(default)]
    project_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Callback {
    redirect_url: String,
}

async fn status(bridge: &Bridge) -> Result<Value, PluginFault> {
    let files = bridge
        .call(Method::GET, "/v0/management/auth-files", None, false)
        .await?;
    let accounts: Vec<_> = files.get("files").and_then(Value::as_array).into_iter().flatten()
        .filter(|a| a.get("provider").or_else(||a.get("type")).and_then(Value::as_str).is_some_and(|v| v == "gemini" || v == "gemini-cli" || v == "antigravity"))
        .map(|a|json!({"id":a.get("id").or_else(||a.get("name")),"email":a.get("email"),"status":a.get("status"),"disabled":a.get("disabled"),"unavailable":a.get("unavailable")})).collect();
    let models = gemini_models(&bridge.call(Method::GET, "/v1/models", None, true).await?);
    Ok(
        json!({"running":true,"pid":bridge.process_id(),"baseUrl":format!("{}/v1",bridge.base_url()),"dataDirectory":bridge.directory,"accounts":accounts,"models":models,"pendingLoginUrl":bridge.oauth_url,"pendingLoginProvider":bridge.oauth_provider,"lastLoginError":bridge.last_login_error,"engineVersion":"8.0.60","googleModuleVersion":"1.0.5-personal.3","defaultLoginProvider":"antigravity"}),
    )
}

async fn inner(
    call: &TypedCall<ManagementRequest>,
    bridge: &mut Bridge,
) -> Result<Value, PluginFault> {
    if !call.request.query.is_empty() {
        return Err(fault("此插件接口不接受查询参数"));
    }
    match (call.request.method.as_str(), call.request.path.as_str()) {
        ("GET", "api/status") => {
            if !call.payload.is_empty() {
                return Err(fault("状态查询不能包含正文"));
            }
            bridge.ensure_started().await?;
            status(bridge).await
        }
        ("POST", "api/login") => {
            let request: Login =
                serde_json::from_slice(&call.payload).map_err(|_| fault("Google 登录参数无效"))?;
            if request.project_id.len() > 256 || request.project_id.chars().any(char::is_control) {
                return Err(fault("Google Cloud 项目标识无效"));
            }
            if request.provider == crate::oauth::LoginProvider::Antigravity
                && !request.project_id.trim().is_empty()
            {
                return Err(fault(
                    "Antigravity 自动处理关联项目，请将 Cloud 项目 ID 留空",
                ));
            }
            bridge.ensure_started().await?;
            if bridge.oauth_state.is_some() {
                return Err(fault("已有授权正在等待，请先完成或取消"));
            }
            let mut body = json!({"provider":request.provider.key(),"is_webui":true,"proxy_url":bridge.settings.proxy_url});
            if !request.project_id.trim().is_empty() {
                body["project_id"] = json!(request.project_id.trim());
            }
            let result = bridge
                .call(
                    Method::POST,
                    "/v8/management/oauth/auth-url",
                    Some(body),
                    false,
                )
                .await?;
            let state = result
                .get("state")
                .and_then(Value::as_str)
                .ok_or_else(|| fault("Google 登录响应缺少状态"))?;
            let auth_url = result
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| fault("Google 登录响应缺少授权地址"))?;
            let parsed = url::Url::parse(auth_url).map_err(|_| fault("Google 授权地址无效"))?;
            if parsed.scheme() != "https" || parsed.host_str() != Some("accounts.google.com") {
                return Err(fault("引擎返回了非 Google 官方授权地址"));
            }
            let redirect = parsed
                .query_pairs()
                .find(|(key, _)| key == "redirect_uri")
                .map(|(_, value)| value.into_owned())
                .ok_or_else(|| fault("Google 授权地址缺少本机回调"))?;
            crate::oauth::validate_callback(
                &redirect,
                state,
                &format!("{redirect}?state={state}"),
            )?;
            bridge.oauth_state = Some(state.to_owned());
            bridge.oauth_url = Some(auth_url.to_owned());
            bridge.oauth_redirect = Some(redirect);
            bridge.oauth_provider = Some(request.provider);
            bridge.last_login_error = None;
            Ok(json!({"url":auth_url,"status":"waiting"}))
        }
        ("POST", "api/open-login") => {
            let url = bridge
                .oauth_url
                .as_deref()
                .ok_or_else(|| fault("请先生成 Google 授权链接"))?;
            crate::process::open_google_login(url)
                .map_err(|_| fault("无法打开默认浏览器，请复制授权链接手动打开"))?;
            Ok(json!({"message":"已在 Windows 默认浏览器打开 Google 授权页"}))
        }
        ("POST", "api/poll") => {
            let result = bridge.poll_oauth().await?;
            let state = result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("wait");
            let message = if state == "error" {
                let message = crate::oauth::diagnostic_message(
                    result.get("error").and_then(Value::as_str).unwrap_or(""),
                );
                bridge.last_login_error = Some(message.clone());
                message
            } else {
                String::new()
            };
            if state == "ok" || state == "error" {
                bridge.reset_oauth();
            }
            // 授权错误不回传可能包含登录材料的引擎原始响应
            Ok(json!({"status":state,"message":message}))
        }
        ("POST", "api/callback") => {
            let callback: Callback =
                serde_json::from_slice(&call.payload).map_err(|_| fault("回调地址格式无效"))?;
            crate::oauth::validate_callback(
                bridge
                    .oauth_redirect
                    .as_deref()
                    .ok_or_else(|| fault("请先生成 Google 授权链接"))?,
                bridge
                    .oauth_state
                    .as_deref()
                    .ok_or_else(|| fault("请先生成 Google 授权链接"))?,
                &callback.redirect_url,
            )?;
            let provider = bridge
                .oauth_provider
                .ok_or_else(|| fault("当前授权缺少登录通道"))?;
            bridge
                .call(
                    Method::POST,
                    "/v8/management/oauth/callback",
                    Some(json!({"provider":provider.key(),"redirect_url":callback.redirect_url})),
                    false,
                )
                .await?;
            Ok(json!({"message":"已提交 Google 回调，请等待授权结果"}))
        }
        ("POST", "api/cancel") => {
            let state = bridge.oauth_state.clone();
            bridge.reset_oauth();
            bridge.last_login_error = None;
            if let Some(state) = state {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("state", &state)
                    .finish();
                bridge
                    .call(
                        Method::DELETE,
                        &format!("/v8/management/oauth/session?{query}"),
                        None,
                        false,
                    )
                    .await?;
            }
            Ok(json!({"message":"已取消等待中的授权"}))
        }
        ("POST", "api/connect") => {
            bridge.ensure_started().await?;
            let models = gemini_models(&bridge.call(Method::GET, "/v1/models", None, true).await?);
            gateway::connect(&call.host, bridge, &models).await
        }
        _ => Err(fault("插件管理接口不存在")),
    }
}

pub async fn handle(
    call: TypedCall<ManagementRequest>,
    bridge: &mut Bridge,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    let (status, value) = match inner(&call, bridge).await {
        Ok(value) => (200, value),
        Err(error) => (400, json!({"error":{"message":error.message}})),
    };
    Ok(TypedReply::new(ManagementResponse {
        status,
        content_type: "application/json".into(),
        headers: vec![],
    })
    .with_payload(serde_json::to_vec(&value).map_err(|_| fault("插件响应编码失败"))?))
}
