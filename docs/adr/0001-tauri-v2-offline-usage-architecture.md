# ADR 0001：Chronolume 2.0 离线分析架构

- 状态：已接受
- 日期：2026-07-10
- 范围：2.0 全量重写

## 决策

应用采用 Tauri 2 + Rust + React + TypeScript + Vite。Rust 端以少量深 Module 承担复杂行为，Tauri command 仅作参数校验与序列化 Adapter；前端只查询后端聚合和服务端分页结果。

稳定 Interface 如下：

- `CodexSource`：`detect`、`plan`、`stream`。实现只读访问 `~/.codex`，隐藏 state DB、JSONL 和 logs DB 的差异。
- `UsageIndexer`：`sync`、`status`、`cancel`。内部完成增量计划、解析、去重、断点、事务、归档移动、重放和保留策略。
- `UsageStore`：打开/迁移分析库并提供事务性存储能力；WAL、prepared statement、批写和完整性检查不泄漏给调用者。
- `UsageQuery`：Dashboard、趋势、热力图、项目、会话、模型、工具和诊断查询；所有筛选、聚合、粒度选择和分页在 SQL/Rust 端完成。
- `Pricing`：模型规范化、匹配、覆盖、更新预览、应用和重算。
- `Activity`：接收仅存在于当前栈帧的原始工具参数，返回工具名、类别和计数；原始参数、命令、工具文件路径和代码不得越过该 Seam。

生产文件源和合成测试源是 `CodexSource` Seam 的两个 Adapter。测试通过与生产调用方相同的 Interface 验证行为，不为内部 reader 建立对外转发层。

## 数据来源优先级

1. 会话 JSONL 是 Token 分类、模型切换、任务生命周期、工具调用和事件时间的首选来源。
2. `state_5.sqlite.threads` 提供会话 ID、归档状态、工作区、模型提供方、总 Token 回退值和 rollout 定位。永不选择 `first_user_message`、`preview` 或原始 `title`。
3. `logs_2.sqlite.logs` 当前仅以 `rowid/id` 高水位确认增量状态，不选择 `feedback_log_body`、`file` 或其他正文列。
4. 同一事实冲突时按上述顺序取值，并记录不含原文的完整性状态和解析警告。

会话列表需要的标题由应用按日期和稳定短 ID 合成，避免把可能源自提示词的 Codex 标题保存到分析库。

## Schema

v2 分析库独立放在 `%LOCALAPPDATA%\Chronolume\v2\chronolume-v2.sqlite3`，至少包含：

- `schema_migrations`、`source_files`、`sync_runs`
- `workspaces`、`sessions`
- `usage_events`、`tool_events`
- `session_model_segments`、`activity_segments`
- `daily_usage_rollups`、`daily_tool_rollups`
- `model_prices`、`app_settings`

所有时间戳保存为 UTC 毫秒；每日汇总同时保存本地日期、时区 ID 和 UTC 日边界，以正确处理 DST。成本字段可为空，未匹配价格必须表示为“未定价”，不能写成虚假 `$0`。

`source_files` 保存 Codex 根目录下的规范化相对路径、来源类型、大小、mtime 纳秒、安全字节偏移、最后完整换行偏移、会话 ID、当前原始模型/计价模型/提供方、累计 Token、解析器版本、状态和脱敏错误码。源文件 ID 在 active/archived 移动时保持稳定；派生事件使用稳定唯一键保证重复同步幂等。

## 隐私边界

- 不读取或持久化提示词、助手回复、推理正文、会话正文、工具输出正文、源代码、凭据或 `auth.json`。
- 不持久化工具参数、命令正文或工具调用涉及的文件路径。工具参数只能以借用值进入 `Activity`，分类后立即丢弃。
- 为实现工作区导航、恢复索引和用户主动选择的完整路径导出，可以保存工作区路径及 Codex 数据源相对路径；两者与工具调用路径严格区分。
- 数据库、缓存、日志及匿名/完整路径两种导出都必须通过唯一敏感标记测试。
- 价格更新是唯一可选联网功能，只能由用户点击触发，并在应用前显示来源、时间与差异。

## 保留策略

- `usage_events`、`tool_events` 等事件级明细保留最近 90 天。
- 删除明细前，必须在同一事务中合并永久 `daily_*_rollups`，并保证只汇总完整本地日。
- 会话摘要、工作区摘要、模型段、活动段、每日汇总和模型汇总永久保留。
- 改价直接用保留的模型维度与 Token 计数重算会话和每日汇总，不重新读取原始 JSONL。

## 性能策略

- 有索引启动时先从 v2 SQLite 返回最近快照，源检测和同步在后台执行；UI 主线程不读文件或数据库。
- JSONL 从最后完整换行的安全字节偏移续读；普通追加禁止从文件头恢复状态。
- 截断、替换或解析器升级时，只事务性重放受影响文件并替换其派生记录。
- 首次导入采用有界单写者流式循环和逐来源事务；内存中只保留当前记录/小批状态，不把文件或全部事件一次性载入。真实 4.17 GB 导入已验证吞吐和内存目标，因此没有为追求并发而增加磁盘争抢。
- SQLite 启用 WAL、`busy_timeout`、外键、prepared statement 和面向筛选/时间/分页的复合索引。
- 趋势查询最多返回屏幕需要的小时/日/周点；前端不得接收数万事件后自行聚合。
- 提供可重复 benchmark，测量冷/热启动、首次索引、单文件追加、SQL P50/P95、峰值/空闲内存和数据库大小。

## 被拒绝的方案

- 继续 Electron + `better-sqlite3`：包体和内存开销高，旧实现对源变化会重新扫描历史数据。
- 复制 cc-switch 的行号水位实现：它仍从文件头逐行恢复状态，无法满足 4 GB 常规刷新目标。
- 将原始事件或工具参数写入分析库后再清洗：违反隐私边界且增加泄漏面。
- 为每个 reader 暴露公共 Interface：会把数据源编排复杂度扩散到 command、UI 和测试，形成浅层 Module。
