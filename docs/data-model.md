# Chronolume 2.0 数据模型

v2 分析数据库位于 `%LOCALAPPDATA%\Chronolume\v2\chronolume-v2.sqlite3`，与旧 Electron 缓存隔离。数据库仅存结构化统计和恢复索引所需元数据，不存会话正文、工具参数、命令或代码。2.0.1 首次启动会迁移旧品牌目录中的 2.0.0 数据库及 WAL sidecar，并可从部分完成的文件改名继续。

| 表 | 保留期 | 主要职责 |
| --- | --- | --- |
| `schema_migrations` | 永久 | 事务性 Schema 版本 |
| `source_files` | 永久 | 安全字节偏移、解析状态和文件身份 |
| `sync_runs` | 永久/可压缩 | 导入进度、性能和错误计数 |
| `workspaces` | 永久 | 规范化、别名和忽略状态 |
| `sessions` | 永久 | 会话级 Token、成本、活跃时间和完整性摘要 |
| `usage_events` | 90 天 | Token delta 明细 |
| `tool_events` | 90 天 | 已脱敏工具名与类别明细 |
| `session_model_segments` | 永久 | 会话级模型切换摘要，不含正文 |
| `activity_segments` | 永久 | 会话级生命周期/估算摘要，不含事件参数 |
| `session_daily_usage` | 永久 | 会话/日期/模型中间汇总，支持精准筛选和改价 |
| `session_daily_tool` | 永久 | 会话/日期/工具类别中间汇总 |
| `daily_usage_rollups` | 永久 | 工作区/提供方/模型/日聚合 |
| `daily_tool_rollups` | 永久 | 工具类别/工具名/日聚合 |
| `model_prices` | 永久 | 内置价格和用户覆盖 |
| `app_settings` | 永久 | 非敏感应用设置 |

`usage_events` 和 `tool_events` 是唯一用于事件分页的表，按本地日边界删除 90 天前记录；删除前先重建永久汇总。`session_model_segments` 与 `activity_segments` 属于会话摘要，只保存模型/时间/计数，不保存消息、工具参数、命令或文件路径。

`source_files` 保存来源相对路径、类型、大小、mtime 纳秒、前缀哈希、安全偏移、完整行偏移、日志 rowid 水位、当前模型/提供方、累计 Token、解析器版本和脱敏错误码。所有表使用严格约束、外键和面向时间/筛选/分页的索引；完整定义以 [0001_initial.sql](../src-tauri/migrations/0001_initial.sql) 为准。
