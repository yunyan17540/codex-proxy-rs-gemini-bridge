//! Gemini 管理插件的公开 SDK 会话与实例生命周期
#![cfg_attr(windows, windows_subsystem = "windows")]

mod accounts;
mod engine;
mod gateway;
mod management;
mod oauth;
mod process;
mod relay;

use gateway_plugin_sdk::client::{
    Empty, PluginBuilder, PluginSession, SessionConfig, TypedReply, methods,
};
use std::sync::Arc;
use tokio::sync::Mutex;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    process::initialize_windows_environment()?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let session = PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        SessionConfig::default(),
    )
    .await?;
    let bridge = Arc::new(Mutex::new(
        engine::Bridge::new(session.handshake())
            .map_err(|error| std::io::Error::other(error.message))?,
    ));
    let management_bridge = Arc::clone(&bridge);
    let plugin = PluginBuilder::from_json(include_bytes!("../plugin.json"))?
        .management(management::registration(), move |call| {
            let bridge = Arc::clone(&management_bridge);
            async move { management::handle(call, &mut *bridge.lock().await).await }
        })?
        .on(methods::RECONCILE, move |_call| {
            let bridge = Arc::clone(&bridge);
            async move {
                bridge.lock().await.ensure_started().await?;
                Ok(TypedReply::new(Empty {}))
            }
        })?
        .build()?;
    session.run(plugin).await?;
    Ok(())
}
