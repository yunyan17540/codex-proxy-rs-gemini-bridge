//! 受管本机 Gemini 引擎、私有登录材料与有限的管理接口

use crate::{
    gateway::fault,
    process::{EngineProcess, protect_directory},
};
use gateway_plugin_sdk::{Handshake, PluginFault};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, net::TcpListener, path::PathBuf, time::Duration};
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub data_directory: String,
    pub port: u16,
    pub proxy_url: String,
}

#[derive(Deserialize, Serialize)]
struct Keys {
    client: String,
    management: String,
}

pub struct Bridge {
    pub instance_id: String,
    pub settings: Settings,
    pub directory: PathBuf,
    package: PathBuf,
    client: Client,
    process: Option<EngineProcess>,
    keys: Option<Keys>,
    pub oauth_state: Option<String>,
    pub oauth_url: Option<String>,
    pub oauth_redirect: Option<String>,
    pub oauth_provider: Option<crate::oauth::LoginProvider>,
    pub last_login_error: Option<String>,
    oauth_poll: Option<JoinHandle<Result<Value, PluginFault>>>,
}

impl Bridge {
    pub fn new(handshake: &Handshake) -> Result<Self, PluginFault> {
        let settings: Settings = serde_json::from_value(handshake.configuration.clone())
            .map_err(|_| fault("插件配置不完整"))?;
        if !PathBuf::from(&settings.data_directory).is_absolute() || settings.port < 1024 {
            return Err(fault("账号目录需要绝对路径，端口必须在 1024–65535 之间"));
        }
        if handshake.instance_id.is_empty()
            || !handshake
                .instance_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(fault("插件实例标识无效"));
        }
        if !settings.proxy_url.is_empty() {
            let proxy = url::Url::parse(&settings.proxy_url)
                .map_err(|_| fault("Google 出站代理地址无效"))?;
            if !["http", "https", "socks5", "socks5h"].contains(&proxy.scheme())
                || proxy.host().is_none()
            {
                return Err(fault("Google 出站代理需要 HTTP 或 SOCKS5 地址"));
            }
        }
        let executable = std::env::current_exe().map_err(|_| fault("无法定位插件安装目录"))?;
        let package = executable
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| fault("插件目录结构无效"))?
            .to_path_buf();
        let directory = PathBuf::from(&settings.data_directory).join(&handshake.instance_id);
        let client = Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| fault("无法初始化本机连接"))?;
        Ok(Self {
            instance_id: handshake.instance_id.clone(),
            settings,
            directory,
            package,
            client,
            process: None,
            keys: None,
            oauth_state: None,
            oauth_url: None,
            oauth_redirect: None,
            oauth_provider: None,
            last_login_error: None,
            oauth_poll: None,
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.settings.port)
    }
    pub fn client_key(&self) -> Result<&str, PluginFault> {
        self.keys
            .as_ref()
            .map(|k| k.client.as_str())
            .ok_or_else(|| fault("Gemini 引擎尚未启动"))
    }

    pub async fn ensure_started(&mut self) -> Result<(), PluginFault> {
        if let Some(process) = &mut self.process {
            if process
                .running()
                .map_err(|_| fault("无法读取 Gemini 引擎状态"))?
            {
                return Ok(());
            }
            self.process = None;
            self.reset_oauth();
        }
        let listener = TcpListener::bind(("127.0.0.1", self.settings.port)).map_err(|error| {
            fault(&format!(
                "无法绑定 Gemini 本机端口 {}（Windows {}）",
                self.settings.port,
                error.raw_os_error().unwrap_or(0)
            ))
        })?;
        fs::create_dir_all(&self.directory).map_err(|_| fault("无法创建账号数据目录"))?;
        protect_directory(&self.directory)
            .map_err(|_| fault("无法保护账号目录，请检查目录权限"))?;
        let keys_path = self.directory.join("bridge-keys.json");
        let keys = if keys_path.exists() {
            serde_json::from_slice(&fs::read(&keys_path).map_err(|_| fault("无法读取本机密钥"))?)
                .map_err(|_| fault("本机密钥文件损坏"))?
        } else {
            let keys = Keys {
                client: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
                management: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            };
            fs::write(
                &keys_path,
                serde_json::to_vec(&keys).map_err(|_| fault("本机密钥编码失败"))?,
            )
            .map_err(|_| fault("无法保存本机密钥"))?;
            keys
        };
        let config = json!({
            "config-version":8,
            "server":{"host":"127.0.0.1","port":self.settings.port},
            "management":{"allow-remote":false,"secret-key":keys.management,"disable-control-panel":true,"disable-auto-update-panel":true},
            "access":{"api-keys":[keys.client]},
            "oauth":{"auth-dir":self.directory.join("auth")},
            "requests":{"proxy-url":self.settings.proxy_url,"retry":0},
            "plugins":{"enabled":true,"dir":self.package.join("engine/plugins"),"configs":{"gemini-cli":{"enabled":true}}},
            "observability":{"logs":{"logging-to-file":false,"request-log":false,"error-logs-max-files":10}}
        });
        let config_path = self.directory.join("config.yaml");
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&config).map_err(|_| fault("引擎配置编码失败"))?,
        )
        .map_err(|_| fault("无法写入引擎配置"))?;
        drop(listener);
        self.keys = Some(keys);
        self.process = Some(
            EngineProcess::spawn(
                &self.package.join("engine/cli-proxy-api.exe"),
                &config_path,
                &self.directory,
            )
            .map_err(|_| fault("Gemini 引擎启动失败"))?,
        );
        for _ in 0..40 {
            if self
                .call(Method::GET, "/v8/management/plugins", None, false)
                .await
                .is_ok()
            {
                return Ok(());
            }
            if let Some(process) = &mut self.process
                && !process.running().map_err(|_| fault("无法读取引擎进程"))?
            {
                self.process = None;
                return Err(fault("Gemini 引擎启动后退出"));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        self.process = None;
        Err(fault("Gemini 引擎启动超时"))
    }

    pub async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        inference_key: bool,
    ) -> Result<Value, PluginFault> {
        let keys = self
            .keys
            .as_ref()
            .ok_or_else(|| fault("Gemini 引擎尚未启动"))?;
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url()))
            .bearer_auth(if inference_key {
                &keys.client
            } else {
                &keys.management
            });
        if let Some(body) = body {
            request = request.json(&body);
        }
        Self::read_json(request).await
    }

    async fn read_json(request: reqwest::RequestBuilder) -> Result<Value, PluginFault> {
        let mut response = request.send().await.map_err(|error| {
            fault(if error.is_timeout() {
                "本机 Gemini 引擎请求超时，请重试授权"
            } else {
                "无法连接本机 Gemini 引擎"
            })
        })?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| fault("Gemini 引擎响应中断"))?
        {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(fault("Gemini 管理响应过大"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(fault(&format!(
                "Gemini 引擎操作失败（HTTP {}）",
                status.as_u16()
            )));
        }
        serde_json::from_slice(&bytes).map_err(|_| fault("Gemini 引擎响应格式无效"))
    }

    /// Google 首次初始化会执行多个 API 请求；后台等待，保持页面刷新和取消可响应。
    pub async fn poll_oauth(&mut self) -> Result<Value, PluginFault> {
        let state = self
            .oauth_state
            .as_deref()
            .ok_or_else(|| fault("当前没有等待中的 Google 授权"))?;
        if self
            .oauth_poll
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            let task = self
                .oauth_poll
                .take()
                .ok_or_else(|| fault("Google 授权任务已结束"))?;
            return task
                .await
                .map_err(|_| fault("Google 授权检查中断，请重新授权"))?;
        }
        if self.oauth_poll.is_none() {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("state", state)
                .finish();
            let key = self
                .keys
                .as_ref()
                .ok_or_else(|| fault("Gemini 引擎尚未启动"))?;
            let request = self
                .client
                .get(format!(
                    "{}/v8/management/oauth/status?{query}",
                    self.base_url()
                ))
                .bearer_auth(&key.management)
                .timeout(Duration::from_secs(105));
            self.oauth_poll = Some(tokio::spawn(Self::read_json(request)));
        }
        Ok(json!({"status":"wait"}))
    }

    pub fn reset_oauth(&mut self) {
        if let Some(task) = self.oauth_poll.take() {
            task.abort();
        }
        self.oauth_state = None;
        self.oauth_url = None;
        self.oauth_redirect = None;
        self.oauth_provider = None;
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().map(EngineProcess::id)
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.reset_oauth();
    }
}

pub fn gemini_models(value: &Value) -> Vec<String> {
    let mut models: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .filter(|id| id.starts_with("gemini-") && id.len() <= 256)
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn slow_oauth_poll_does_not_block_management_and_can_be_canceled() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener");
        let port = listener.local_addr().expect("test address").port();
        let (accepted, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("test connection");
            let mut bytes = [0; 4096];
            let _ = stream.read(&mut bytes).await;
            let _ = accepted.send(());
            tokio::time::sleep(Duration::from_millis(250)).await;
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"status\":\"wait\"}").await;
        });
        let mut bridge = Bridge {
            instance_id: "test-instance".into(),
            settings: Settings {
                data_directory: "C:\\test".into(),
                port,
                proxy_url: String::new(),
            },
            directory: PathBuf::new(),
            package: PathBuf::new(),
            client: Client::builder().no_proxy().build().expect("test HTTP"),
            process: None,
            keys: Some(Keys {
                client: "test-client".into(),
                management: "test-management".into(),
            }),
            oauth_state: Some("test-state".into()),
            oauth_url: None,
            oauth_redirect: None,
            oauth_provider: None,
            last_login_error: None,
            oauth_poll: None,
        };
        assert_eq!(bridge.poll_oauth().await.expect("start")["status"], "wait");
        ready.await.expect("request received");
        let result = tokio::time::timeout(Duration::from_millis(50), bridge.poll_oauth()).await;
        assert!(
            result.is_ok(),
            "management request must not wait for the slow OAuth exchange"
        );
        bridge.reset_oauth();
        assert!(bridge.oauth_poll.is_none() && bridge.oauth_state.is_none());
        assert!(
            bridge.poll_oauth().await.is_err(),
            "canceled login must not accept a late poll result"
        );
        server.await.expect("test server finished");
    }
    #[test]
    fn model_sync_excludes_other_providers_and_duplicates() {
        assert_eq!(
            gemini_models(
                &json!({"data":[{"id":"gpt-test"},{"id":"gemini-flash"},{"id":"gemini-flash"},{"id":"claude-test"}]})
            ),
            vec!["gemini-flash"]
        );
    }
}
