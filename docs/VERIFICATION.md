# 验证范围

0.1.6 在 Windows x64、codex-proxy-rs 3.41.2 上增加了独立账号管理，并通过：

- Rust 格式检查、严格 Clippy、10 项单元测试及 Release 构建；测试包含真实 loopback HTTP、SSE 转发、429 保留和删除密钥失效
- Vue / TypeScript 检查与生产构建
- 使用发行包的 20 项隔离宿主检查：独立连接、重复同步、旧合并记录停用保留、调度设置保留、定向授权链接、删除竞态阻止及单账号清理
- 使用同一个 CLIProxyAPI 8.0.60 二进制的 6 项本机路由检查：账号前缀隔离、故障不在引擎内换号、未知账号拒绝及已有无前缀客户端兼容

这些检查使用新建的隔离数据库、人工凭据和本机模拟上游。它们验证了账号管理和转发规则；重新授权检查只生成官方授权 URL 并核对目标，未实际登录 Google 或完成新 OAuth 回调，也未发起真实生图或视频请求。引擎及 Google 模块二进制沿用 0.1.5，本版没有修改或重新验证 Google 服务端登录和媒体能力。

0.1.5 已在 Windows x64、codex-proxy-rs 3.41.2 上通过：

- Rust 格式检查、严格 Clippy、5 项单元测试、Release 构建
- Go 模块测试、go vet 与 Windows c-shared 构建
- Vue / TypeScript 检查与生产构建
- 隔离宿主安装、管理及转发检查 19 项，授权通道检查 10 项，生命周期检查 11 项
- 定时恢复、宿主退出回收，以及测试 PostgreSQL / Redis / 网关的清理

0.1.5 的发行验证使用人工账号与本机模拟模型，不包含真实账号凭据。Code Assist OAuth 客户端配置在构建时提供，Antigravity 引擎和授权逻辑沿用原实现。

0.1.4 已用真实 Antigravity 账号验证授权、令牌刷新、`gemini-3-flash` 指定账号推理，以及现有网关普通和流式 Responses；得到真实助手回复和 `response.completed`。该记录作为集成基线，未将它表述为 0.1.5 的真实账号调用。

其余公布模型、Linux、macOS、Windows ARM64 与其他宿主版本没有逐一验证。新增通道按钮的浏览器视觉验收受测试环境限制，前端构建与管理 API 流程已验证。
