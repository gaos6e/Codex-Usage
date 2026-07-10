# Codex Usage 2.0 架构

2.0 使用 Tauri 2、Rust、React、TypeScript 和 Vite，面向 OpenAI Codex 本地数据，离线优先且不实现代理、账号或在线配额。

核心架构决策见 [ADR 0001](adr/0001-tauri-v2-offline-usage-architecture.md)。实现遵循“深 Module、小 Interface、调用方与测试共用同一 Seam”：

```text
~/.codex (只读)
       │
  CodexSource
       │ stream(change)
  UsageIndexer ── Activity / Pricing
       │ transaction
   UsageStore (v2 SQLite)
       │
   UsageQuery
       │ validated DTO
 Tauri Commands
       │
 React + TanStack Query
```

启动时 UI 先查询现有 SQLite 快照并立即可交互；索引器随后在后台执行增量同步。`usage-sync-completed` 会失效 `dashboard`、`workspaces`、`sessions`、`session-detail`、`usage-events`、`models`、`heatmap`、`tools` 和 `diagnostics` Query key；失败事件只刷新同步状态。首次导入同样不阻塞 UI，并提供真实文件/字节进度、速度、取消、断点续传和错误恢复。

Tauri command 仅校验 DTO、调用对应 Module 并序列化结果。文件读取、SQLite、聚合、定价和活动分类均不在 UI 线程执行；前端只接收聚合点、分页行和脱敏诊断码。
