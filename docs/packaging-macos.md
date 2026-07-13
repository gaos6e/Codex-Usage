# macOS 打包、签名与发布门禁

Chronolume 2.1.0 的 macOS 目标是 macOS 12.0+ Universal 应用，同时包含 `arm64` 与 `x86_64` slice。macOS bundle identifier 为 `com.gaos6e.chronolume`；应用保持非沙盒，不启用 `com.apple.security.app-sandbox`，也不在仓库中保存证书、Team ID 或公证凭据。

## 阶段 A：未签名候选

没有 Apple Developer 凭据和自有 Mac 时，`Build and upload unsigned macOS candidate` 工作流会在 Release 发布后自动运行，也可手动指定已有 Release 标签运行。它会：

1. 在 `macos-14` runner 安装 `aarch64-apple-darwin` 与 `x86_64-apple-darwin`。
2. 执行前端检查、Rust 测试和锁文件驱动的许可证生成。
3. 通过 Tauri 锁定 CLI 的 `universal-apple-darwin` 与 `--no-sign` 构建 `.app`/`.dmg`。
4. 用 `lipo` 检查两个 slice，用 `plutil` 检查 Bundle ID、2.1.0、最低系统 12.0 和应用名。
5. `hdiutil verify` 并只读挂载 DMG，检查应用结构、唯一主二进制，以及 `Info.plist` 声明的 ICNS 或 Asset Catalog 图标资源。
6. 使用一个不存在 `.codex` 的临时 HOME 启动挂载后的应用，确认进程不会在启动阶段退出，并断言数据库只出现在 `Library/Application Support/Chronolume/v2`、没有 bundle-ID 嵌套目录，再安全终止和卸载。
7. 扫描包内数据库、WAL、日志和 `auth.json`，生成 `SHA256SUMS-macos.txt` 与 `verification-macos.txt`。

本地 macOS 可复现命令：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm ci
pwsh ./scripts/generate-third-party-licenses.ps1 -CargoTarget aarch64-apple-darwin,x86_64-apple-darwin
npm run tauri:build:macos -- --no-sign --ci
bash ./scripts/verify-macos-bundle.sh
```

未签名 `.app`/`.dmg` 只用于测试，文件名必须带 `unsigned`。工作流会把通过验证的候选追加到对应 GitHub Release，同时保留短期 Actions artifact；这些资产不视为受信任的正式 macOS 分发，也不向用户提供绕过 Gatekeeper 的操作说明。

### 2.1.0 阶段 A 构建记录

2026-07-11 的 [GitHub Actions run 29150578335](https://github.com/gaos6e/Chronolume/actions/runs/29150578335) 在 `macos-14-arm64` runner image `20260629.0180.1` 上完成。前端 28 项测试、Rust 55 项测试、双目标 Release 编译、Universal `lipo` 检查、Info.plist、图标、DMG 完整性/只读挂载、无 `.codex` 临时 HOME 启动、精确数据库目录和敏感文件扫描均通过。

上传的内部 artifact 为 `Chronolume-unsigned-macos-universal`，artifact ID `8248081248`，大小 17,078,098 字节，GitHub artifact digest 为 `sha256:d1797f361cdec135a0bdc803af30dbe1802d332efe6588769fd580479e175fc9`；artifact 内另含 `.app.zip`、`.dmg`、二者 SHA-256 清单和验证记录。该记录只证明阶段 A 的未签名 runner 候选，不替代 Developer ID、公证、staple、Gatekeeper 或外部 Mac 真机验收。

## 阶段 B：Developer ID 与公证

正式发布必须等待以下外部条件全部满足：

- 加入 Apple Developer Program 并获得 `Developer ID Application` 证书；
- 在受保护 GitHub Environment `macos-release` 配置人工审批和 secrets；
- 至少一次 Apple Silicon 原生验收，并通过 Rosetta 验证 x86_64 slice，或另用 Intel Mac 验证；
- 用户明确批准正式发布。

Environment secrets：

| Secret | 用途 |
| --- | --- |
| `APPLE_CERTIFICATE` | base64 编码的 Developer ID `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` 密码 |
| `KEYCHAIN_PASSWORD` | CI 临时 keychain 密码 |
| `APPLE_SIGNING_IDENTITY` | Developer ID Application identity |
| `APPLE_API_ISSUER` | App Store Connect issuer ID |
| `APPLE_API_KEY` | App Store Connect key ID |
| `APPLE_API_PRIVATE_KEY` | `.p8` 私钥内容 |
| `APPLE_TEAM_ID` | Apple Developer Team ID |

`Release signed and notarized dual-platform build` 只能从 `main` 通过 `workflow_dispatch` 启动，要求外部验收记录和明确批准短语。工作流先在 Windows runner 重建并 smoke NSIS/便携 ZIP，再由受保护的 macOS job 使用临时 keychain，执行 `codesign --verify`、`spctl --assess`、Apple 公证、staple 与 `xcrun stapler validate`。最终 Release 同时上传 Windows 与 macOS 资产及各自 SHA-256/验证记录。任务无论成功或失败都会删除临时证书、API key 与 keychain。

当前实现遵循 [Tauri macOS code signing](https://v2.tauri.app/distribute/sign/macos/) 的 App Store Connect API Key 环境变量，并按 [GitHub Actions 安全建议](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions) 将第三方 Action 固定到官方仓库的完整 commit SHA。

## 真机验收记录

正式发布前记录：Chronolume 版本、Mac 芯片、macOS 版本、原生/Rosetta 模式、首次启动、真实 `~/.codex` 只读索引、增量/取消/重建/清空、CSV/JSON/PNG 导出、系统主题、菜单快捷键、关闭/Dock 重开/退出、官方价格检查、`auth.json` 不访问和源数据不修改。当前没有真机记录，因此阶段 B 尚未完成。
