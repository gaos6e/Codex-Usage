# Windows 打包

Chronolume 2.1 使用 Tauri 2 构建原生 Release EXE、当前用户级 NSIS 安装器和便携 ZIP。Windows 继续使用 `com.gaos6e.codexusage`，以保持安装升级和 WebView 偏好连续。打包前先执行标准验证：

```powershell
npm ci
npm run check:versions
npm run typecheck
npm run lint
npm test
npm run build

Set-Location src-tauri
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test
Set-Location ..

npm run tauri:build:windows
.\scripts\build-portable.ps1
.\scripts\smoke-distributions.ps1
```

主要产物：

```text
src-tauri\target\release\chronolume.exe
src-tauri\target\release\bundle\nsis\Chronolume_2.1.0_x64-setup.exe
src-tauri\target\release\bundle\portable\Chronolume-2.1.0-windows-x64-portable.zip
```

便携 ZIP 包含同一个 Release EXE、README、项目 `LICENSE` 和由实际锁文件生成的 `THIRD_PARTY_LICENSES.txt`，不附带分析数据库或用户数据。`build-portable.ps1` 会在压缩前重新执行许可证审计；旧的手写 `THIRD_PARTY_NOTICES.md` 不再是输入。安装器当前未签名，Windows SmartScreen 可能在建立签名信誉前显示警告。运行时目录由 Windows 平台路径解析，不依赖源码仓库位置。

## 2.1.0 验证记录

以下结果来自本分支完成的 Windows Release/NSIS 构建、许可证重生成、便携打包与 `smoke-distributions.ps1`，没有沿用 2.0.2 历史数字。

| 产物 | 字节 | SHA-256 |
| --- | ---: | --- |
| `chronolume.exe` | 10,357,760 | `1ABD2880AE06860C85BAC1A55FC014735DC9CBFE75A2F33AFFBDFAAA47AC3DC9` |
| `Chronolume_2.1.0_x64-setup.exe` | 4,271,138 | `BCF83876B8AD2675B00A22DD58F4467682C811CB5E726397E648E40173898290` |
| `Chronolume-2.1.0-windows-x64-portable.zip` | 4,330,353 | `829E0AE2375BD433FD33A83D8F3A8FB13A216CE43C1AD04B8BE943F8CEE782E2` |

NSIS 静默安装退出码为 0；最新复跑中安装版在 127.60 ms 创建窗口，便携 ZIP 解压版在 95.87 ms 创建窗口。二者均由 smoke 脚本按自身 PID 正常关窗，ZIP 解压目录已安全清除；结构化 smoke JSON 又由发布暂存脚本读取并校验。`THIRD_PARTY_LICENSES.txt` 为 752,490 字节，SHA-256 为 `F470B4F3D31C1B6BE3C93526AD77726E7545A3A64C1F2BD915802F028E21E947`。

## 2.0.2 最终产物（2026-07-11）

| 产物 | 字节 | SHA-256 |
| --- | ---: | --- |
| `chronolume.exe` | 10,357,248 | `C871BADC7EDE00710CCDF33A1236CA29E251B9BEF899F8DC5AEDD966B5CAB693` |
| `Chronolume_2.0.2_x64-setup.exe` | 4,184,150 | `5C0BE952CB871DFE6556090092E560D224FAAD87518B0D45ABD8919535D25A18` |
| `Chronolume-2.0.2-windows-x64-portable.zip` | 4,222,150 | `C90EB5F292D7D020544B6FCE99DC469A5C8EBA9F952363335B3096508F470DBC` |

本轮最终打包由 Tauri/NSIS 正常完成。NSIS 静默安装退出码为 0；安装版在 108.05 ms 创建窗口，便携 ZIP 解压版在 97.40 ms 创建窗口。二者均由测试脚本按自身 PID 正常关窗，ZIP 解压目录已安全清除。桌面 `Chronolume.lnk` 指向本轮安装的 `%LOCALAPPDATA%\Chronolume\chronolume.exe`。

真实 2.0.0 数据迁移前后均为 1,801 个会话、82,569 条保留事件、1,804 个来源检查点、28 条模型价格和 2 项应用设置；迁移后的 SQLite `quick_check` 为 `ok`。旧程序目录、旧快捷方式和旧卸载注册项已按绝对路径定向清理，`~/.codex`、用户导出和 benchmark 未被删除。
