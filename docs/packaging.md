# Windows 打包

Codex Usage 2.0 使用 Tauri 2 构建原生 Release EXE 和当前用户级 NSIS 安装器。打包前先执行标准验证：

```powershell
npm install
npm run typecheck
npm run lint
npm test
npm run build

Set-Location src-tauri
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
Set-Location ..

npm run tauri:build
```

主要产物：

```text
src-tauri\target\release\codex-usage.exe
src-tauri\target\release\bundle\nsis\Codex Usage_2.0.0_x64-setup.exe
src-tauri\target\release\bundle\portable\Codex-Usage-2.0.0-windows-x64-portable.zip
```

便携 ZIP 包含同一个 Release EXE、README 和第三方声明，不附带分析数据库或用户数据。安装器当前未签名，Windows SmartScreen 可能在建立签名信誉前显示警告。运行时目录由 Windows 平台路径解析，不依赖源码仓库位置。

## 2.0.0 最终产物（2026-07-11）

| 产物 | 字节 | SHA-256 |
| --- | ---: | --- |
| `codex-usage.exe` | 10,351,616 | `594E7434352F2AF21D82786B40E5077336A157B5FF032FBB45CC11CF4079F57F` |
| `Codex Usage_2.0.0_x64-setup.exe` | 4,171,589 | `FDE757E4D84477215E53FE417E624A6FEAB13A14824C824789B15C6968D1F320` |
| `Codex-Usage-2.0.0-windows-x64-portable.zip` | 4,219,688 | `4545D41191029FE028847DCB69BE576E90990FBF4DDAAF44EE418B1314C36BF5` |

本轮最终打包由 Tauri/NSIS 正常完成。NSIS 静默安装退出码为 0；安装版在 126.75 ms 创建窗口，便携 ZIP 解压版在 98.97 ms 创建窗口。二者均由测试脚本按自身 PID 正常关窗，ZIP 解压目录已安全清除。桌面 `Codex Usage.lnk` 指向本轮安装的 `%LOCALAPPDATA%\Codex Usage\codex-usage.exe`。
