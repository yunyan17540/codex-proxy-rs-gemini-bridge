# Gemini 谷歌账号插件

为 [codex-proxy-rs](https://github.com/zyycn/codex-proxy-rs) 接入 Google 账号与 Gemini 模型。个人账号使用 Antigravity OAuth，具备 Code Assist Standard / Enterprise 资格的账号可选择对应通道。

**兼容范围：codex-proxy-rs 3.41.2 · Windows x64 · 插件 0.1.5**

[下载安装包](https://github.com/yunyan17540/codex-proxy-rs-gemini-bridge/releases/latest) · [源码构建](docs/BUILD.md) · [验证范围](docs/VERIFICATION.md)

## 安装

在宿主“插件管理”选择 GitHub 来源，填写：

```text
yunyan17540/codex-proxy-rs-gemini-bridge
```

标签留空，选择 Windows x64 的 `.tar.gz` 附件，核对 SHA-256 后确认安装。也可以从 Releases 下载 `.tar.gz` 与 `.sha256`，使用“上传包”安装。GitHub 自动生成的源码 ZIP 不能作为插件安装包。

插件 ID 为 `local.gemini-bridge`，与已有本地安装一致；这是社区插件，与官方发布者无关联。插件以宿主的 Windows 用户身份运行，安装表示信任其原生程序。

## 配置与使用

1. 将“账号数据目录”设为自己的绝对路径，例如 `D:\GeminiBridgeData`，本机中转端口默认 `18764`
2. “Google 出站代理”填写自己可用的 HTTP / SOCKS5 代理；默认 `http://127.0.0.1:7890`，不需要代理时填写空字符串
3. 保存并启用插件，打开“Gemini 谷歌账号”，选择“个人账号 · Antigravity”，生成 Google 授权链接并由本人完成登录
4. 授权完成后刷新状态；需要手动回调时，粘贴本次授权返回的完整本机 URL，插件会核对地址、端口、路径及 state
5. 点击“同步模型并连接”，创建 Gemini 分组与本机 OpenAI 兼容上游；使用分组限制的客户端 Key 还需加入该分组
6. 客户端继续使用现有网关地址和密钥，模型名称以插件实际返回为准，例如 `gemini-3-flash`

网关中生成的上游 Provider 显示为 OpenAI，用于连接本机协议转换引擎。重复同步沿用同一上游，并保留手动调整的名称、启停状态、并发、权重和额外分组。

Code Assist Standard / Enterprise 通道需要具有相应资格的 Google Cloud 项目。个人账号通道自动处理关联项目；Gemini 网页会员或单独新建 Cloud 项目不会赋予商业 Code Assist 资格。账号资格、地区、模型和额度由 Google 决定。

Google 的个人通道说明：[停用说明](https://developers.google.com/gemini-code-assist/docs/deprecations/code-assist-individuals) · [迁移公告](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli)

## 数据与停用

账号令牌、引擎配置与随机本机密钥保存在“账号数据目录 / 插件实例 ID”，限制当前 Windows 用户与 SYSTEM 访问。令牌不返回管理页面，项目不附带任何可用账号或代理订阅。

引擎只监听 `127.0.0.1`，管理接口与模型接口使用不同密钥，常规请求日志关闭。插件启动时管理引擎，停用或进程异常退出时回收引擎；停用会中断尚未完成的 Gemini 请求，账号数据保留。

卸载前停用插件创建的网关上游，并检查客户端 Key 的分组范围。数据目录需要自行备份和管理。

## 组件与许可

插件源码使用 MIT 许可；包含公开 SDK 的 Apache-2.0 许可及第三方 MIT 声明。

- CLIProxyAPI `8.0.60`：官方 Windows x64 主程序，内置 Antigravity OAuth
- Code Assist 模块 `1.0.5-personal.3`：基于 `cpa-plugin-gemini-cli v1.0.5` 源码本地构建，并非上游官方发行物
- `gateway-plugin-sdk`：来自 codex-proxy-rs `3.41.2`，Rust 源码保持原样

来源与摘要见 [SOURCE-PROVENANCE.json](SOURCE-PROVENANCE.json)，第三方声明见 [legal/NOTICE.md](legal/NOTICE.md)，模块修改说明见 [google-module/LOCAL-CHANGES.md](google-module/LOCAL-CHANGES.md)。
