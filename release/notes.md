# v0.1.5

首次公开发布 Gemini 谷歌账号桥接插件，适配 codex-proxy-rs 3.41.2 / Windows x64。

- 个人 Google 账号通过 Antigravity OAuth 接入 Gemini
- 提供 Code Assist Standard / Enterprise 授权入口
- 自动管理本机转换引擎，支持手动回调、取消授权及状态刷新
- 同步实际 Gemini 模型，创建独立分组和受管上游
- Code Assist OAuth 客户端配置由构建环境提供，公开源码不包含客户端 ID 和密钥值

发行包包含本机引擎与依赖，使用时无需编译。插件 ID 保持 `local.gemini-bridge`，可用于已有本地安装的 GitHub 更新来源。

真实 `gemini-3-flash` 及网关普通、流式 Responses 已在 0.1.4 验证；0.1.5 已通过 Rust / Go / Vue 构建、5 项单元测试和 40 项隔离宿主检查。其他模型、平台与宿主版本尚未逐一验证。
