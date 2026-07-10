# Third-Party Notices

Codex Usage 使用 `package-lock.json` 与 `src-tauri/Cargo.lock` 中列出的开源依赖。各依赖保留其原许可证；分发时相应许可证元数据随包管理锁文件和上游项目提供。

## cc-switch

- Project: <https://github.com/farion1231/cc-switch>
- Reviewed commit: `98ccde0050f33a1bc8b16b96287a0b6f582c5d12`
- License: MIT
- Copyright: Copyright (c) 2025 Jason Young
- Reviewed files: `UsageDashboard.tsx`, `UsageHero.tsx`, `UsageTrendChart.tsx`, `UsageDateRangePicker.tsx`, `RequestLogTable.tsx`, `ModelStatsTable.tsx`, `ProviderStatsTable.tsx`, `session_usage_codex.rs`, `usage_stats.rs`, `usage_rollup.rs`, and database schema.

Codex Usage 的视觉层级与趋势图交互参考了上述文件，但 Rust 数据模型、解析、查询和 React 组件均在本仓库重新实现；未复制 cc-switch 品牌或 Logo。

MIT permission notice:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files, to deal in the Software without restriction, subject to inclusion of the copyright and permission notice. The software is provided “AS IS”, without warranty.

完整条款见上游 `LICENSE`。

## AiMaMi

- Project: <https://github.com/borawong/AiMaMi>
- Reviewed commit: `297c7af56f10fb371b77bc9b6b65aa320afcbe7e`
- License: Apache License 2.0
- Reviewed material: dashboard/heatmap UI、analytics types and feature concepts.

AiMaMi 的公开提交缺少完整 Analytics 实现，因此只作为 Bento 视觉、热力图和功能概念参考；未假设或复制缺失实现，也未复制品牌资源。完整 Apache-2.0 条款见上游 `LICENSE` 和 <https://www.apache.org/licenses/LICENSE-2.0>。
