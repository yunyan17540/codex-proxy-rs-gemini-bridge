# 源码构建

安装包自带所需二进制，使用插件时无需开发工具。源码构建仅支持 Windows x64。

准备 Rust 1.99.1、MSVC / Windows SDK、Python 3、Go 1.26.0、MinGW-w64 GCC、Node.js 24+ 和 pnpm。将 `cargo`、`go`、`gcc`、`node` 与 `pnpm` 加入 PATH。

源码中的 SDK 已按目标宿主版本附带；前端依赖使用锁文件，CLIProxyAPI 下载后按固定 SHA-256 校验。Code Assist 模块从随仓库提供的源码编译。

Code Assist 模块的 OAuth 客户端配置通过构建环境提供，不写入源码。先在当前 PowerShell 会话设置 `GEMINI_OAUTH_CLIENT_ID` 与 `GEMINI_OAUTH_CLIENT_SECRET`，值为自己获准使用的 Google OAuth 客户端配置。构建脚本临时生成忽略入库的 Go 配置文件，并在结束后删除。个人 Antigravity 通道由固定版本的 CLIProxyAPI 引擎提供。

在仓库根目录执行：

```powershell
python .\fetch-engine.py --proxy ''
.\build-google.ps1
pnpm --dir frontend install --frozen-lockfile
.\build.ps1
```

需要下载代理时，将 `--proxy ''` 替换为自己的代理地址。

从 codex-proxy-rs `v3.41.2` 构建匹配的打包工具：

```powershell
cargo install --locked --git https://github.com/zyycn/codex-proxy-rs.git --tag v3.41.2 codex-proxy-plugin-cli --root .tools
.\package.ps1 -Packager '.\.tools\bin\cpr-plugin.exe'
```

`dist-package/` 中生成 `.tar.gz` 安装包和 `.sha256`，不要将数据目录、账号令牌或引擎运行配置放入发行物。GitHub Releases 发布安装包与校验文件后，宿主才能通过 GitHub 来源安装。
