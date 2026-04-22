# Requirements Summary

Codex Usage is a local-first Windows desktop app for inspecting local Codex usage by workspace, time range, and metric view.

Implemented first-version surfaces:

- Dashboard with workspace selector, exact time presets, custom range, daily/weekly chart, Time/Tokens toggle, metric cards, run list, refresh, and export actions.
- Project Detail with workspace trend, runs, model token distribution, peak days, and recent sessions.
- Settings with Codex data directory, archived sessions, logs toggle, auto-refresh, idle gap cap, theme, and path aliases.
- Diagnostics with detected sources, schema columns, parser warnings, cache status, app data path, and log path.

Privacy boundary:

- Reads `state_5.sqlite`, session JSONL timestamps/metadata, and best-effort `logs_2.sqlite` token fields.
- Does not read or export `first_user_message` or JSONL conversation content.
- Does not scan `auth.json`, credential backups, sandbox secrets, or arbitrary workspace source files.
