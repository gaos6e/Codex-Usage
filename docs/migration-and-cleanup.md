# 2.0 迁移与清理

## 数据目录

Codex Usage 2.0 使用独立分析库：

```text
%LOCALAPPDATA%\CodexUsage\v2\codex-usage-v2.sqlite3
```

它不会读取或迁移 1.x `settings.json`、JSON cache、Electron profile 或 Squirrel packages。首次启动从 `~/.codex` 只读重建完整索引。

## 安全边界

清理流程只允许影响仓库内旧 Electron 文件，以及 `%LOCALAPPDATA%\CodexUsage` 内明确识别出的 1.x cache、profile、packages、backup、旧 EXE 和 `settings.json`。以下内容永远不在清理范围：

- `%USERPROFILE%\.codex` 及其中的 `state_5.sqlite`、`logs_2.sqlite`、`sessions`、`archived_sessions` 和备份；
- `%LOCALAPPDATA%\CodexUsage\v2`；
- `%LOCALAPPDATA%\CodexUsage\exports` 和用户选择的其他导出目录；
- 当前仓库之外的无关文件。

执行删除前必须把每个绝对路径解析到允许根目录，先列出目标，再使用同一个 PowerShell 进程和 `Remove-Item -LiteralPath`。只有在测试、真实导入、Windows 构建和 smoke test 全部通过后才能执行。

## 回滚

2.0 分析库可随时从诊断页清空并重新索引；该操作只删除分析派生数据，保留模型价格和 UI 设置，也不写入 `~/.codex`。1.x 与 2.0 没有长期兼容层。

## 已执行结果（2026-07-10）

在新实现测试、4.17 GB 导入、查询基准、Release/NSIS 构建和首次 smoke test 通过后，按上述顺序列出并验证绝对路径，再删除仓库内 Electron/Forge/Webpack 源码、测试、配置、生成物及 `%LOCALAPPDATA%\CodexUsage` 的 1.x 项。随后再次完成全套验证与打包。

清理前后活动 JSONL 数量均为 1,796，`state_5.sqlite` 均为 12,996,608 字节，`logs_2.sqlite` 当时均为 131,006,464 字节。清理后 `%LOCALAPPDATA%\CodexUsage` 只保留用户 `exports`、性能 `benchmarks` 和新应用 `v2`；没有删除或移动 `~/.codex`、用户导出或 v2 分析库。
