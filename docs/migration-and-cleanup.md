# Chronolume 2.0 迁移与清理

## 数据目录

Chronolume 2.0.1 使用独立分析库：

```text
%LOCALAPPDATA%\Chronolume\v2\chronolume-v2.sqlite3
```

从 Codex Usage 2.0.0 升级时，首次启动会在打开 SQLite 前，将旧品牌目录中的整个 `v2` 目录及数据库/WAL/SHM 文件移动到新位置并改名。迁移在同一卷内完成，不覆盖已经存在的 Chronolume 数据库；因此历史索引、模型价格和应用设置可以连续使用，无需重新读取 JSONL。

它不会读取或迁移 1.x `settings.json`、JSON cache、Electron profile 或 Squirrel packages。没有 2.0 数据库时，首次启动仍从 `~/.codex` 只读重建完整索引。

## 安全边界

清理流程只允许影响仓库内旧 Electron 文件，以及 `%LOCALAPPDATA%\CodexUsage` 内明确识别出的 1.x cache、profile、packages、backup、旧 EXE 和 `settings.json`。以下内容永远不在清理范围：

- `%USERPROFILE%\.codex` 及其中的 `state_5.sqlite`、`logs_2.sqlite`、`sessions`、`archived_sessions` 和备份；
- `%LOCALAPPDATA%\Chronolume\v2`；
- 品牌迁移完成前的 `%LOCALAPPDATA%\CodexUsage\v2`；
- `%LOCALAPPDATA%\CodexUsage\exports` 和用户选择的其他导出目录；
- 当前仓库之外的无关文件。

执行删除前必须把每个绝对路径解析到允许根目录，先列出目标，再使用同一个 PowerShell 进程和 `Remove-Item -LiteralPath`。只有在测试、真实导入、Windows 构建和 smoke test 全部通过后才能执行。

## 回滚

2.0 分析库可随时从诊断页清空并重新索引；该操作只删除分析派生数据，保留模型价格和 UI 设置，也不写入 `~/.codex`。1.x 与 2.0 没有长期兼容层。Chronolume 继续使用原稳定 bundle identifier，仅用于安装升级和 WebView 偏好连续；该内部标识不作为用户可见品牌。

## 已执行结果（2026-07-10）

在新实现测试、4.17 GB 导入、查询基准、Release/NSIS 构建和首次 smoke test 通过后，按上述顺序列出并验证绝对路径，再删除仓库内 Electron/Forge/Webpack 源码、测试、配置、生成物及 `%LOCALAPPDATA%\CodexUsage` 的 1.x 项。随后再次完成全套验证与打包。

清理前后活动 JSONL 数量均为 1,796，`state_5.sqlite` 均为 12,996,608 字节，`logs_2.sqlite` 当时均为 131,006,464 字节。旧版清理后 `%LOCALAPPDATA%\CodexUsage` 只保留用户 `exports`、性能 `benchmarks` 和当时的新应用 `v2`；没有删除或移动 `~/.codex`、用户导出或 v2 分析库。2.0.1 品牌迁移只移动该 `v2` 目录，旧 `exports` 和 benchmark 仍按用户数据边界保留。
