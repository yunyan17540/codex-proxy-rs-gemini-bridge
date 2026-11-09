# 第三方组件

本插件集成按发布方 SHA-256 校验的 CLIProxyAPI v8.0.60 官方 Windows x64 主程序
Google 模块以 cpa-plugin-gemini-cli v1.0.5 的 MIT 许可源码为基础，修正个人账号发现与初始化后本地构建，版本为 1.0.5-personal.3；它不是上游官方发行物

- https://github.com/router-for-me/CLIProxyAPI/releases/tag/v8.0.60
- https://github.com/router-for-me/cpa-plugin-gemini-cli/releases/tag/v1.0.5
- 修复参考：https://github.com/router-for-me/cpa-plugin-gemini-cli/pull/4 （已关闭且未合并）
- 官方流程依据：https://github.com/google-gemini/gemini-cli/blob/249d51c2631ba1a36b71b0f2c50195abbe4100ce/packages/core/src/code_assist/setup.ts

个人账号使用 CLIProxyAPI 内置的 Antigravity 授权；Code Assist 模块仅用于相应 Standard / Enterprise 资格，账号可用模型由 Google 决定
该模块完成登录后会尝试为关联项目启用 Gemini Code Assist EXPERIMENTAL 预览渠道
Google 账号令牌保存在本机配置的私有数据目录，不返回浏览器，不发送至第三方中转网站

插件使用 codex-proxy-rs v3.41.2 的公开 gateway-plugin-sdk，Apache-2.0 许可随包保留
CLIProxyAPI 与 Gemini 模块的 MIT 许可随包保留
