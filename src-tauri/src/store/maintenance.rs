use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use super::UsageStore;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettingsRecord {
    pub id: String,
    pub alias: Option<String>,
    pub ignored: bool,
}

impl UsageStore {
    pub fn update_workspace_settings(
        &self,
        workspace_id: &str,
        alias: Option<&str>,
        ignored: bool,
    ) -> AppResult<WorkspaceSettingsRecord> {
        let alias = alias.map(str::trim).filter(|value| !value.is_empty());
        if alias.is_some_and(|value| value.chars().count() > 80) {
            return Err(AppError::new(
                "invalid_alias",
                "工作区别名不能超过 80 个字符",
            ));
        }
        self.with_writer(|transaction| {
            let updated = transaction.execute(
                "UPDATE workspaces SET alias = ?2, ignored = ?3, updated_at_ms = ?4 WHERE id = ?1",
                params![
                    workspace_id,
                    alias,
                    i64::from(ignored),
                    Utc::now().timestamp_millis()
                ],
            )?;
            if updated == 0 {
                return Err(AppError::new("workspace_not_found", "工作区不存在"));
            }
            Ok(WorkspaceSettingsRecord {
                id: workspace_id.to_string(),
                alias: alias.map(str::to_string),
                ignored,
            })
        })
    }

    pub fn clear_analysis(&self) -> AppResult<()> {
        self.clear_index_tables(true)
    }

    pub(crate) fn reset_index(&self) -> AppResult<()> {
        self.clear_index_tables(false)
    }

    fn clear_index_tables(&self, clear_sync_history: bool) -> AppResult<()> {
        self.with_writer(|transaction| {
            for table in [
                "daily_tool_rollups",
                "daily_usage_rollups",
                "session_daily_tool",
                "session_daily_usage",
                "tool_events",
                "usage_events",
                "activity_segments",
                "session_model_segments",
                "sessions",
                "source_files",
                "workspaces",
            ] {
                transaction.execute(&format!("DELETE FROM {table}"), [])?;
            }
            if clear_sync_history {
                transaction.execute("DELETE FROM sync_runs", [])?;
            }
            Ok(())
        })?;
        self.with_reader(|connection| {
            connection.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")?;
            Ok(())
        })
    }

    pub fn app_setting(&self, key: &str) -> AppResult<Option<String>> {
        self.with_reader(|connection| {
            connection
                .query_row(
                    "SELECT value_json FROM app_settings WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(AppError::from)
        })
    }

    pub fn save_app_setting(&self, key: &str, value_json: &str) -> AppResult<()> {
        if key.is_empty()
            || key.len() > 120
            || serde_json::from_str::<serde_json::Value>(value_json).is_err()
        {
            return Err(AppError::new("invalid_setting", "设置键或 JSON 值无效"));
        }
        self.with_writer(|transaction| {
            transaction.execute(
                "INSERT INTO app_settings (key, value_json, updated_at_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                     updated_at_ms = excluded.updated_at_ms",
                params![key, value_json, Utc::now().timestamp_millis()],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_analysis_never_touches_source_files_or_prices() {
        let source_directory = tempfile::tempdir().unwrap();
        let source_path = source_directory.path().join("rollout.jsonl");
        std::fs::write(&source_path, b"private source remains").unwrap();
        let store = UsageStore::open_in_memory().unwrap();
        store.seed_builtin_prices().unwrap();
        store
            .with_writer(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, normalized_path, display_name, ignored, created_at_ms, updated_at_ms)
                     VALUES ('w', 'C:/repo', 'repo', 0, 0, 0)",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO sessions (
                        id, workspace_id, synthetic_title, model_provider, latest_model_raw,
                        primary_model_raw, integrity_status, parser_version, updated_at_ms
                     ) VALUES ('s', 'w', 'Session', 'openai', 'gpt-5', 'gpt-5', 'complete', 1, 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        store.clear_analysis().unwrap();
        assert_eq!(
            std::fs::read(&source_path).unwrap(),
            b"private source remains"
        );
        store
            .with_reader(|connection| {
                let sessions: i64 =
                    connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
                let prices: i64 =
                    connection
                        .query_row("SELECT COUNT(*) FROM model_prices", [], |row| row.get(0))?;
                assert_eq!(sessions, 0);
                assert!(prices > 0);
                Ok(())
            })
            .unwrap();
    }
}
