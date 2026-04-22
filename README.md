# Codex Usage

Codex Usage is a local-first Windows desktop app that reads local Codex usage data and shows usage by workspace, time range, and metric view.

## Features

- Dashboard with Time and Tokens views.
- Workspace grouping from normalized `threads.cwd`.
- Time presets: Today, Last 7 days, Last 30 days, Last 90 days, All time, and Custom range.
- Estimated agent time from JSONL event timestamps with a configurable idle-gap cap.
- Total tokens from `state_5.sqlite`, with best-effort breakdown from `logs_2.sqlite`.
- Project Detail, Settings, Diagnostics, Light/Dark themes, CSV/JSON/image export.
- Privacy boundary: thread titles are allowed, but conversation content and `first_user_message` are not read or exported.

## Technology Stack

- Electron Forge + Webpack + TypeScript
- React
- `better-sqlite3`
- Recharts
- lucide-react
- Vitest and Playwright

## Install

```powershell
cd D:\codex-usage-windows-app
npm install
```

## Development

```powershell
npm start
```

The app defaults to `%USERPROFILE%\.codex`. You can choose another `.codex` directory in Settings.

## Test

```powershell
npm run typecheck
npm test
npm run lint
```

E2E tests are configured as a manual smoke-test placeholder:

```powershell
$env:RUN_E2E = "1"
npm run test:e2e
```

## Package

```powershell
npm run make
```

Expected Windows installer output:

```text
D:\codex-usage-windows-app\out\make\squirrel.windows\x64\Codex Usage Setup.exe
```

Forge also creates a `.nupkg` and `RELEASES` file in the same folder.

## Runtime Data

Application settings, cache, logs, and default exports are stored under:

```text
%LOCALAPPDATA%\CodexUsage
```

The packaged app does not depend on `C:\base` or `D:\codex-usage-windows-app` at runtime.

## Common Issues

- If `state_5.sqlite` is missing, open Settings and choose a valid Codex data directory.
- If token breakdown is unavailable, the app still shows total tokens from `threads.tokens_used`.
- If the installer is unsigned, Windows SmartScreen may show a warning.
- If native module rebuilding fails during packaging, install the Visual Studio Build Tools workload for desktop C++ and rerun `npm run make`.
