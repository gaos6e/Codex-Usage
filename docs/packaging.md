# Windows 打包

Chronolume 2.0 使用 Tauri 2 构建原生 Release EXE 和当前用户级 NSIS 安装器。打包前先执行标准验证：

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
src-tauri\target\release\chronolume.exe
src-tauri\target\release\bundle\nsis\Chronolume_2.0.1_x64-setup.exe
src-tauri\target\release\bundle\portable\Chronolume-2.0.1-windows-x64-portable.zip
```

便携 ZIP 包含同一个 Release EXE、README 和第三方声明，不附带分析数据库或用户数据。安装器当前未签名，Windows SmartScreen 可能在建立签名信誉前显示警告。运行时目录由 Windows 平台路径解析，不依赖源码仓库位置。

## 2.0.1 最终产物（2026-07-11）

| 产物 | 字节 | SHA-256 |
| --- | ---: | --- |
| `chronolume.exe` | 10,357,760 | `5C216EB976EC3C5AE370744AD47D4C0E0E4B719628BC0CB4D23C96568B09EAEA` |
| `Chronolume_2.0.1_x64-setup.exe` | 4,187,245 | `96FE2863421DA84E13A28AECDCAC6879E8B1015A6985A13B7CBA3649E772D721` |
| `Chronolume-2.0.1-windows-x64-portable.zip` | 4,222,077 | `D9C855A7D2A4F6CF7131D2B5ECD2E7C3CF0D04B74FDD60833CB6665FE6CF21ED` |

本轮最终打包由 Tauri/NSIS 正常完成。NSIS 静默安装退出码为 0；安装版在 116.33 ms 创建窗口，便携 ZIP 解压版在 91.19 ms 创建窗口。二者均由测试脚本按自身 PID 正常关窗，ZIP 解压目录已安全清除。桌面 `Chronolume.lnk` 指向本轮安装的 `%LOCALAPPDATA%\Chronolume\chronolume.exe`。

真实 2.0.0 数据迁移前后均为 1,801 个会话、82,569 条保留事件、1,804 个来源检查点、28 条模型价格和 2 项应用设置；迁移后的 SQLite `quick_check` 为 `ok`。旧程序目录、旧快捷方式和旧卸载注册项已按绝对路径定向清理，`~/.codex`、用户导出和 benchmark 未被删除。
