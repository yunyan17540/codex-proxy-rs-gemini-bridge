//! 只监听 loopback 的账号连接；每条连接有独立密钥和唯一模型前缀，无隐藏换号。
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, Response, StatusCode},
};
use reqwest::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::task::JoinHandle;

#[derive(Clone)]
struct Route {
    key: String,
    enabled: bool,
}
#[derive(Clone)]
struct RelayState {
    client: Client,
    engine: String,
    engine_key: String,
    routes: Arc<RwLock<BTreeMap<String, Route>>>,
}
pub struct AccountRelay {
    state: RelayState,
    task: JoinHandle<()>,
    #[cfg(test)]
    bound_port: u16,
}

impl AccountRelay {
    pub fn start(port: u16, engine: String, engine_key: String) -> Result<Self, std::io::Error> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
        #[cfg(test)]
        let bound_port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(listener)?;
        let state = RelayState {
            client: Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(600))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(std::io::Error::other)?,
            engine,
            engine_key,
            routes: Arc::new(RwLock::new(BTreeMap::new())),
        };
        let app = Router::new().fallback(forward).with_state(state.clone());
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            state,
            task,
            #[cfg(test)]
            bound_port,
        })
    }
    pub fn set(&self, id: &str, key: &str, enabled: bool) {
        self.state.routes.write().expect("route lock").insert(
            id.into(),
            Route {
                key: key.into(),
                enabled,
            },
        );
    }
    pub fn remove(&self, id: &str) {
        self.state.routes.write().expect("route lock").remove(id);
    }
}
impl Drop for AccountRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn error(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"error":{"message":message,"type":"gemini_account_error"}}).to_string(),
        ))
        .expect("static response")
}

fn endpoint(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/accounts/")?;
    let (id, path) = rest.split_once("/v1/")?;
    if !crate::accounts::valid_id(id) {
        return None;
    }
    if ![
        "models",
        "responses",
        "responses/compact",
        "chat/completions",
        "completions",
        "images/generations",
    ]
    .contains(&path)
    {
        return None;
    }
    Some((id, path))
}

pub fn scoped_model(id: &str, model: &str) -> Option<String> {
    if !crate::accounts::valid_id(id)
        || !model.starts_with("gemini-")
        || model.len() > 256
        || model.contains('/')
        || model.chars().any(char::is_control)
    {
        return None;
    }
    Some(format!("{}/{model}", crate::accounts::prefix(id)))
}

async fn forward(State(state): State<RelayState>, request: Request<Body>) -> Response<Body> {
    let Some((id, path)) =
        endpoint(request.uri().path()).map(|(a, b)| (a.to_owned(), b.to_owned()))
    else {
        return error(StatusCode::NOT_FOUND, "账号接口不存在");
    };
    if request.uri().query().is_some() {
        return error(StatusCode::BAD_REQUEST, "账号接口不接受查询参数");
    }
    let route = state.routes.read().expect("route lock").get(&id).cloned();
    let Some(route) = route else {
        return error(StatusCode::NOT_FOUND, "此 Google 账号连接已删除");
    };
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if supplied != format!("Bearer {}", route.key) {
        return error(StatusCode::UNAUTHORIZED, "账号连接密钥无效");
    }
    if !route.enabled {
        return error(
            StatusCode::UNAUTHORIZED,
            "此 Google 账号已停用或需要重新授权",
        );
    }
    let method = request.method().clone();
    if !(method == Method::GET && path == "models" || method == Method::POST && path != "models") {
        return error(StatusCode::METHOD_NOT_ALLOWED, "不支持此账号操作");
    }
    let mut outgoing = state
        .client
        .request(method.clone(), format!("{}/v1/{path}", state.engine))
        .bearer_auth(&state.engine_key);
    for name in ["accept", "idempotency-key", "x-request-id"] {
        if let Some(value) = request.headers().get(name) {
            outgoing = outgoing.header(name, value.clone());
        }
    }
    if method == Method::POST {
        let Ok(Ok(bytes)) = tokio::time::timeout(
            Duration::from_secs(30),
            to_bytes(request.into_body(), 64 * 1024 * 1024),
        )
        .await
        else {
            return error(StatusCode::PAYLOAD_TOO_LARGE, "账号请求过大或接收超时");
        };
        let Ok(mut payload) = serde_json::from_slice::<Value>(&bytes) else {
            return error(StatusCode::BAD_REQUEST, "请使用 JSON 模型请求");
        };
        let Some(model) = payload
            .get("model")
            .and_then(Value::as_str)
            .and_then(|m| scoped_model(&id, m))
        else {
            return error(
                StatusCode::BAD_REQUEST,
                "此连接仅接受未带前缀的 Gemini 模型",
            );
        };
        payload["model"] = json!(model);
        outgoing = outgoing.json(&payload);
    }
    let Ok(response) = outgoing.send().await else {
        return error(StatusCode::BAD_GATEWAY, "无法连接本机 Gemini 引擎");
    };
    if path == "models" && response.status().is_success() {
        let Ok(value) = response.json::<Value>().await else {
            return error(StatusCode::BAD_GATEWAY, "模型目录格式无效");
        };
        let prefix = format!("{}/", crate::accounts::prefix(&id));
        let models: Vec<_> = value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| {
                let name = model.get("id")?.as_str()?.strip_prefix(&prefix)?;
                if !name.starts_with("gemini-") {
                    return None;
                }
                let mut model = model.clone();
                model["id"] = json!(name);
                Some(model)
            })
            .collect();
        return Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"object":"list","data":models}).to_string(),
            ))
            .expect("static response");
    }
    let mut result = Response::builder().status(response.status());
    for name in [
        "content-type",
        "retry-after",
        "x-request-id",
        "cache-control",
    ] {
        if let Some(value) = response.headers().get(name) {
            result = result.header(name, value);
        }
    }
    result
        .body(Body::from_stream(response.bytes_stream()))
        .expect("upstream response")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn live_relay_pins_payload_streams_errors_and_invalidates_deleted_keys() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let captured = Arc::clone(&calls);
        let mock = Router::new().fallback(move |request: Request<Body>| {
            let captured = Arc::clone(&captured);
            async move {
                assert_eq!(
                    request.headers().get("authorization").unwrap(),
                    "Bearer fixture-engine"
                );
                let body = to_bytes(request.into_body(), 4096).await.unwrap();
                let value: Value = serde_json::from_slice(&body).unwrap();
                captured.lock().unwrap().push(value.clone());
                if value["input"] == "fail" {
                    Response::builder()
                        .status(429)
                        .header("retry-after", "60")
                        .body(Body::from("fixture quota"))
                        .unwrap()
                } else {
                    Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from("data: {\"type\":\"response.completed\"}\n\n"))
                        .unwrap()
                }
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, mock).await.unwrap();
        });
        let relay = AccountRelay::start(0, base, "fixture-engine".into()).unwrap();
        relay.set("account-a", "key-a", true);
        relay.set("account-b", "key-b", true);
        let url = format!(
            "http://127.0.0.1:{}/accounts/account-a/v1/responses",
            relay.bound_port
        );
        let client = Client::builder().no_proxy().build().unwrap();
        let reply = client
            .post(&url)
            .bearer_auth("key-a")
            .json(&json!({"model":"gemini-test","input":"ok","stream":true}))
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 200);
        assert!(reply.text().await.unwrap().contains("response.completed"));
        assert_eq!(
            calls.lock().unwrap()[0]["model"],
            "gb-account-a/gemini-test"
        );
        let reply = client
            .post(&url)
            .bearer_auth("key-a")
            .json(&json!({"model":"gemini-test","input":"fail"}))
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 429);
        assert_eq!(reply.headers()["retry-after"], "60");
        assert_eq!(
            calls.lock().unwrap().len(),
            2,
            "no transparent retry or cross-account fallback"
        );
        assert_eq!(
            client
                .post(&url)
                .bearer_auth("key-b")
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        relay.remove("account-a");
        assert_eq!(
            client
                .post(&url)
                .bearer_auth("key-a")
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(calls.lock().unwrap().len(), 2);
        task.abort();
    }
    #[test]
    fn account_routes_cannot_inject_other_account_or_management_paths() {
        assert_eq!(
            endpoint("/accounts/a123/v1/responses"),
            Some(("a123", "responses"))
        );
        assert!(endpoint("/accounts/../v1/responses").is_none());
        assert!(endpoint("/accounts/a123/v1/../../v8/management/credentials").is_none());
        assert_eq!(
            scoped_model("a123", "gemini-test").as_deref(),
            Some("gb-a123/gemini-test")
        );
        assert!(scoped_model("a123", "gb-other/gemini-test").is_none());
        assert!(scoped_model("a123", "gpt-test").is_none());
    }
}
