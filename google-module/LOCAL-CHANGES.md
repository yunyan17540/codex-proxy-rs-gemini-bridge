# 本地个人账号修正

基础源码：router-for-me/cpa-plugin-gemini-cli v1.0.5，保留原 MIT 许可
构建版本：1.0.5-personal.3

修复参考 [PR #4](https://github.com/router-for-me/cpa-plugin-gemini-cli/pull/4) 的提交 `92d565eaf85e1f0c5553c306d0105d9de5278763`。该提案已关闭且未合并，不能表述为上游官方修复。已对照 Google 官方 CLI 的 [setup.ts](https://github.com/google-gemini/gemini-cli/blob/249d51c2631ba1a36b71b0f2c50195abbe4100ce/packages/core/src/code_assist/setup.ts) 核对。

- 没有显式项目时，以账号级 Code Assist 查询代替枚举并选取任意 Cloud 项目
- 已开通账号使用服务返回的受管项目，跳过重复初始化
- 免费层初始化不传用户项目；其他层保留明确指定的授权项目
- 未完成操作通过版本化 GET 接口轮询，支持取消与超时
- 只保存经 Code Assist 确认的项目，保留 Google 对订阅和账号验证的拒绝
- 额外修复完成操作的 error 响应：发生失败时不保存显式项目为成功账号
- 未分配项目时保留 Google 返回的资格原因，不一律归为项目缺失；仅保存固定枚举、状态码和字段形状的诊断，排除令牌、邮箱、项目及响应原文

修复前，5 项 HTTP 边界回归检查在原版源码上失败。修复后运行完整 Go 测试及 go vet，并在 Windows x64 上以 c-shared 构建。测试使用人工 HTTP 响应，没有使用真实 Google 令牌；不能据此宣称用户账号已成功授权。
