# Privacy Notes

The app is local-first and read-only against Codex data sources.

Default behavior:

- No background upload.
- No remote telemetry.
- No conversation body display.
- No export without explicit user action.
- Export can use full local paths or anonymized workspace names.

Data stored under `%LOCALAPPDATA%\CodexUsage`:

- `settings.json`
- `cache\summary-cache.json`
- `logs\main.log`
- optional export files chosen by the user

The cache stores computed summaries and source fingerprints. It should not contain prompt text, tool output, credentials, or workspace source content.
