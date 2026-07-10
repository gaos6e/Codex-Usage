mod jsonl;
mod logs_db;
mod state_db;

use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::activity::{OperationKind, ToolCategory};
use crate::error::{AppError, AppResult};

pub const PARSER_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Session,
    ArchivedSession,
    StateDb,
    LogsDb,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::ArchivedSession => "archived_session",
            Self::StateDb => "state_db",
            Self::LogsDb => "logs_db",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub kind: SourceKind,
    pub exists: bool,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCapabilities {
    pub statuses: Vec<SourceStatus>,
    pub can_read_session_tokens: bool,
    pub can_read_session_metadata: bool,
    pub can_read_logs: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SourceCursor {
    pub safe_offset: u64,
    pub complete_line_offset: u64,
    pub current_model_raw: Option<String>,
    pub current_pricing_model_id: Option<String>,
    pub model_provider: Option<String>,
    pub cumulative_input_tokens: u64,
    pub cumulative_cached_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    pub cumulative_reasoning_tokens: u64,
    pub relevant_event_ordinal: u64,
    pub open_task_turn_id: Option<String>,
    pub open_task_started_at_ms: Option<i64>,
    pub logs_rowid_watermark: u64,
}

#[derive(Debug, Clone)]
pub struct SourceCheckpoint {
    pub source_key: String,
    pub relative_path: String,
    pub kind: SourceKind,
    pub session_id: Option<String>,
    pub file_size: u64,
    pub mtime_ns: u128,
    pub prefix_hash: String,
    pub parser_version: i64,
    pub cursor: SourceCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeAction {
    Append,
    Replay,
    MetadataOnly,
    Missing,
}

#[derive(Debug, Clone)]
pub struct SourceChange {
    pub(crate) source_key: String,
    pub(crate) relative_path: String,
    pub(crate) path: PathBuf,
    pub(crate) kind: SourceKind,
    pub(crate) session_id: Option<String>,
    pub(crate) action: ChangeAction,
    pub(crate) file_size: u64,
    pub(crate) mtime_ns: u128,
    pub(crate) prefix_hash: String,
    pub(crate) cursor: SourceCursor,
}

impl SourceChange {
    pub fn source_key(&self) -> &str {
        &self.source_key
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn kind(&self) -> SourceKind {
        self.kind
    }

    pub fn action(&self) -> ChangeAction {
        self.action
    }

    pub fn expected_bytes(&self) -> u64 {
        match self.action {
            ChangeAction::Append => self.file_size.saturating_sub(self.cursor.safe_offset),
            ChangeAction::Replay => self.file_size,
            ChangeAction::MetadataOnly | ChangeAction::Missing => 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChangePlan {
    pub changes: Vec<SourceChange>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRecord {
    SessionMetadata(SessionMetadata),
    StateSessionMetadata(StateSessionMetadata),
    LogsAdvanced(LogsAdvanced),
    ModelChanged(ModelChanged),
    Usage(UsageEvent),
    Activity(ActivitySegment),
    Tool(ToolEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSessionMetadata {
    pub session_id: String,
    pub cwd: String,
    pub model_provider: String,
    pub model_raw: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogsAdvanced {
    pub rowid_watermark: u64,
    pub rows_seen: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadata {
    pub event_ordinal: u64,
    pub occurred_at_ms: Option<i64>,
    pub session_id: String,
    pub cwd: Option<String>,
    pub model_provider: Option<String>,
    pub legacy_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChanged {
    pub byte_offset: u64,
    pub event_ordinal: u64,
    pub occurred_at_ms: Option<i64>,
    pub model_provider: String,
    pub model_raw: String,
    pub pricing_model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEvent {
    pub byte_offset: u64,
    pub event_ordinal: u64,
    pub occurred_at_ms: i64,
    pub model_provider: String,
    pub model_raw: String,
    pub pricing_model_id: String,
    pub input_tokens: u64,
    pub fresh_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivitySegment {
    pub byte_offset: u64,
    pub event_ordinal: u64,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub active_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEvent {
    pub byte_offset: u64,
    pub event_ordinal: u64,
    pub sub_index: u32,
    pub occurred_at_ms: i64,
    pub tool_name: String,
    pub category: ToolCategory,
    pub operation_kind: OperationKind,
}

#[derive(Debug, Clone)]
pub struct StreamOutcome {
    pub cursor: SourceCursor,
    pub bytes_read: u64,
    pub records_emitted: u64,
    pub parse_failures: u64,
    pub incomplete_tail: bool,
    pub cancelled: bool,
}

pub trait CodexSource: Send + Sync {
    fn detect(&self) -> AppResult<SourceCapabilities>;
    fn plan(&self, checkpoints: &[SourceCheckpoint]) -> AppResult<ChangePlan>;
    fn stream(
        &self,
        change: &SourceChange,
        sink: &mut dyn FnMut(ParsedRecord) -> AppResult<()>,
        cancel: &AtomicBool,
    ) -> AppResult<StreamOutcome>;
}

#[derive(Debug, Clone)]
pub struct FsCodexSource {
    root: PathBuf,
}

impl FsCodexSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn session_files(&self) -> AppResult<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        self.collect_jsonl(&self.root.join("sessions"), SourceKind::Session, &mut files)?;
        self.collect_jsonl(
            &self.root.join("archived_sessions"),
            SourceKind::ArchivedSession,
            &mut files,
        )?;
        Ok(files)
    }

    fn database_files(&self) -> Vec<DiscoveredFile> {
        [
            ("state-db", "state_5.sqlite", SourceKind::StateDb),
            ("logs-db", "logs_2.sqlite", SourceKind::LogsDb),
        ]
        .into_iter()
        .filter_map(|(source_key, relative_path, kind)| {
            let path = self.root.join(relative_path);
            let metadata = path.metadata().ok()?;
            Some(DiscoveredFile {
                source_key: source_key.to_string(),
                relative_path: relative_path.to_string(),
                path,
                kind,
                file_size: metadata.len(),
                mtime_ns: modified_ns(&metadata),
            })
        })
        .collect()
    }

    fn collect_jsonl(
        &self,
        directory: &Path,
        kind: SourceKind,
        target: &mut Vec<DiscoveredFile>,
    ) -> AppResult<()> {
        if !directory.is_dir() {
            return Ok(());
        }
        for entry in WalkDir::new(directory).follow_links(false).into_iter() {
            let entry = entry.map_err(|_| AppError::filesystem())?;
            if !entry.file_type().is_file()
                || entry.path().extension().and_then(|value| value.to_str()) != Some("jsonl")
            {
                continue;
            }
            let metadata = entry.metadata().map_err(|_| AppError::filesystem())?;
            let relative_path = normalize_relative_path(
                entry
                    .path()
                    .strip_prefix(&self.root)
                    .map_err(|_| AppError::filesystem())?,
            );
            let stem = entry
                .path()
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(&relative_path)
                .to_ascii_lowercase();
            target.push(DiscoveredFile {
                source_key: format!("jsonl:{stem}"),
                relative_path,
                path: entry.path().to_path_buf(),
                kind,
                file_size: metadata.len(),
                mtime_ns: modified_ns(&metadata),
            });
        }
        Ok(())
    }
}

impl CodexSource for FsCodexSource {
    fn detect(&self) -> AppResult<SourceCapabilities> {
        let session_files = self.session_files()?;
        let status_for = |kind| {
            let matching: Vec<_> = session_files
                .iter()
                .filter(|file| file.kind == kind)
                .collect();
            SourceStatus {
                kind,
                exists: match kind {
                    SourceKind::Session => self.root.join("sessions").is_dir(),
                    SourceKind::ArchivedSession => self.root.join("archived_sessions").is_dir(),
                    _ => false,
                },
                file_count: matching.len() as u64,
                total_bytes: matching.iter().map(|file| file.file_size).sum(),
            }
        };
        let file_status = |kind, name: &str| {
            let path = self.root.join(name);
            let metadata = path.metadata().ok();
            SourceStatus {
                kind,
                exists: metadata.is_some(),
                file_count: u64::from(metadata.is_some()),
                total_bytes: metadata.map_or(0, |value| value.len()),
            }
        };

        let statuses = vec![
            status_for(SourceKind::Session),
            status_for(SourceKind::ArchivedSession),
            file_status(SourceKind::StateDb, "state_5.sqlite"),
            file_status(SourceKind::LogsDb, "logs_2.sqlite"),
        ];
        Ok(SourceCapabilities {
            can_read_session_tokens: statuses[0].exists || statuses[1].exists,
            can_read_session_metadata: statuses[2].exists,
            can_read_logs: statuses[3].exists,
            statuses,
        })
    }

    fn plan(&self, checkpoints: &[SourceCheckpoint]) -> AppResult<ChangePlan> {
        let checkpoint_by_key: HashMap<_, _> = checkpoints
            .iter()
            .map(|checkpoint| (checkpoint.source_key.as_str(), checkpoint))
            .collect();
        let mut seen = HashSet::new();
        let mut changes = Vec::new();

        let mut discovered_files = self.session_files()?;
        discovered_files.extend(self.database_files());
        for discovered in discovered_files {
            seen.insert(discovered.source_key.clone());
            let checkpoint = checkpoint_by_key
                .get(discovered.source_key.as_str())
                .copied();
            let change = match checkpoint {
                None => Some(discovered.as_change(
                    if discovered.kind == SourceKind::LogsDb {
                        ChangeAction::Append
                    } else {
                        ChangeAction::Replay
                    },
                    SourceCursor::default(),
                    if matches!(
                        discovered.kind,
                        SourceKind::Session | SourceKind::ArchivedSession
                    ) {
                        prefix_hash(&discovered.path)?
                    } else {
                        String::new()
                    },
                    None,
                )),
                Some(checkpoint) => plan_existing(discovered, checkpoint)?,
            };
            if let Some(change) = change {
                changes.push(change);
            }
        }

        for checkpoint in checkpoints {
            if !seen.contains(&checkpoint.source_key) {
                changes.push(SourceChange {
                    source_key: checkpoint.source_key.clone(),
                    relative_path: checkpoint.relative_path.clone(),
                    path: self.root.join(&checkpoint.relative_path),
                    kind: checkpoint.kind,
                    session_id: checkpoint.session_id.clone(),
                    action: ChangeAction::Missing,
                    file_size: 0,
                    mtime_ns: 0,
                    prefix_hash: checkpoint.prefix_hash.clone(),
                    cursor: checkpoint.cursor.clone(),
                });
            }
        }

        // 状态库先提供归档与工作区元数据；JSONL 仍按最近优先，使 Dashboard 尽快可用。
        changes.sort_by_key(|change| {
            let priority = match change.kind {
                SourceKind::StateDb => 0,
                SourceKind::Session | SourceKind::ArchivedSession => 1,
                SourceKind::LogsDb => 2,
            };
            (priority, std::cmp::Reverse(change.mtime_ns))
        });
        let total_bytes = changes.iter().map(SourceChange::expected_bytes).sum();
        Ok(ChangePlan {
            changes,
            total_bytes,
        })
    }

    fn stream(
        &self,
        change: &SourceChange,
        sink: &mut dyn FnMut(ParsedRecord) -> AppResult<()>,
        cancel: &AtomicBool,
    ) -> AppResult<StreamOutcome> {
        match change.action {
            ChangeAction::Append | ChangeAction::Replay => match change.kind {
                SourceKind::Session | SourceKind::ArchivedSession => {
                    jsonl::stream_jsonl(change, sink, cancel)
                }
                SourceKind::StateDb => state_db::stream_state_db(change, sink, cancel),
                SourceKind::LogsDb => logs_db::stream_logs_db(change, sink, cancel),
            },
            ChangeAction::MetadataOnly | ChangeAction::Missing => Ok(StreamOutcome {
                cursor: change.cursor.clone(),
                bytes_read: 0,
                records_emitted: 0,
                parse_failures: 0,
                incomplete_tail: false,
                cancelled: false,
            }),
        }
    }
}

#[derive(Debug)]
struct DiscoveredFile {
    source_key: String,
    relative_path: String,
    path: PathBuf,
    kind: SourceKind,
    file_size: u64,
    mtime_ns: u128,
}

impl DiscoveredFile {
    fn as_change(
        &self,
        action: ChangeAction,
        cursor: SourceCursor,
        prefix_hash: String,
        session_id: Option<String>,
    ) -> SourceChange {
        SourceChange {
            source_key: self.source_key.clone(),
            relative_path: self.relative_path.clone(),
            path: self.path.clone(),
            kind: self.kind,
            session_id,
            action,
            file_size: self.file_size,
            mtime_ns: self.mtime_ns,
            prefix_hash,
            cursor,
        }
    }
}

fn plan_existing(
    discovered: DiscoveredFile,
    checkpoint: &SourceCheckpoint,
) -> AppResult<Option<SourceChange>> {
    if matches!(discovered.kind, SourceKind::StateDb | SourceKind::LogsDb) {
        let unchanged = discovered.file_size == checkpoint.file_size
            && discovered.mtime_ns == checkpoint.mtime_ns
            && checkpoint.parser_version == PARSER_VERSION;
        if unchanged {
            return Ok(None);
        }
        let replay = checkpoint.parser_version != PARSER_VERSION
            || discovered.file_size < checkpoint.file_size
            || discovered.kind == SourceKind::StateDb;
        return Ok(Some(discovered.as_change(
            if replay {
                ChangeAction::Replay
            } else {
                ChangeAction::Append
            },
            if replay {
                SourceCursor::default()
            } else {
                checkpoint.cursor.clone()
            },
            String::new(),
            None,
        )));
    }
    let moved =
        discovered.relative_path != checkpoint.relative_path || discovered.kind != checkpoint.kind;
    let unchanged = discovered.file_size == checkpoint.file_size
        && discovered.mtime_ns == checkpoint.mtime_ns
        && checkpoint.cursor.safe_offset >= discovered.file_size
        && checkpoint.parser_version == PARSER_VERSION;
    if unchanged {
        return Ok(moved.then(|| {
            discovered.as_change(
                ChangeAction::MetadataOnly,
                checkpoint.cursor.clone(),
                checkpoint.prefix_hash.clone(),
                checkpoint.session_id.clone(),
            )
        }));
    }

    let current_prefix = prefix_hash(&discovered.path)?;
    let requires_replay = checkpoint.parser_version != PARSER_VERSION
        || discovered.file_size < checkpoint.cursor.safe_offset
        || checkpoint.prefix_hash.is_empty()
        || current_prefix != checkpoint.prefix_hash;
    let action = if requires_replay {
        ChangeAction::Replay
    } else {
        ChangeAction::Append
    };
    let cursor = if requires_replay {
        SourceCursor::default()
    } else {
        checkpoint.cursor.clone()
    };
    Ok(Some(discovered.as_change(
        action,
        cursor,
        current_prefix,
        checkpoint.session_id.clone(),
    )))
}

fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn modified_ns(metadata: &Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

fn prefix_hash(path: &Path) -> AppResult<String> {
    use std::io::{BufRead, Read};

    // 首条 session_meta 在正常追加中不可变。若哈希固定字节窗口，小于窗口的文件
    // 每次追加都会改变哈希并被误判为替换，因此这里只读取首个完整记录。
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut first_line = Vec::with_capacity(4096);
    reader
        .by_ref()
        .take(256 * 1024)
        .read_until(b'\n', &mut first_line)?;
    Ok(blake3::hash(&first_line).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn plans_append_replay_archive_move_and_parser_upgrade() {
        let directory = tempfile::tempdir().unwrap();
        let sessions = directory.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("rollout-abc.jsonl");
        std::fs::write(&path, b"{}\n").unwrap();
        let source = FsCodexSource::new(directory.path());
        let first = source.plan(&[]).unwrap();
        assert_eq!(first.changes[0].action(), ChangeAction::Replay);

        let change = &first.changes[0];
        let metadata = std::fs::metadata(&path).unwrap();
        let checkpoint = SourceCheckpoint {
            source_key: change.source_key.clone(),
            relative_path: change.relative_path.clone(),
            kind: SourceKind::Session,
            session_id: Some("abc".to_string()),
            file_size: metadata.len(),
            mtime_ns: modified_ns(&metadata),
            prefix_hash: change.prefix_hash.clone(),
            parser_version: PARSER_VERSION,
            cursor: SourceCursor {
                safe_offset: metadata.len(),
                complete_line_offset: metadata.len(),
                ..SourceCursor::default()
            },
        };

        let mut append = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(append, "{{}}").unwrap();
        drop(append);
        let appended = source.plan(std::slice::from_ref(&checkpoint)).unwrap();
        assert_eq!(appended.changes[0].action(), ChangeAction::Append);

        let upgraded = SourceCheckpoint {
            parser_version: PARSER_VERSION - 1,
            ..checkpoint.clone()
        };
        let replayed = source.plan(&[upgraded]).unwrap();
        assert_eq!(replayed.changes[0].action(), ChangeAction::Replay);

        let archived = directory.path().join("archived_sessions");
        std::fs::create_dir_all(&archived).unwrap();
        let archived_path = archived.join("rollout-abc.jsonl");
        std::fs::rename(&path, &archived_path).unwrap();
        let moved = source.plan(&[checkpoint]).unwrap();
        assert_eq!(moved.changes.len(), 1);
        assert_eq!(moved.changes[0].kind(), SourceKind::ArchivedSession);
        assert_ne!(moved.changes[0].action(), ChangeAction::Missing);
    }

    #[test]
    fn detects_available_sources_without_reading_content() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("sessions/2026/07/10")).unwrap();
        std::fs::write(
            directory.path().join("sessions/2026/07/10/a.jsonl"),
            b"sensitive body is not parsed by detect",
        )
        .unwrap();
        std::fs::write(directory.path().join("state_5.sqlite"), b"").unwrap();
        let capabilities = FsCodexSource::new(directory.path()).detect().unwrap();

        assert!(capabilities.can_read_session_tokens);
        assert!(capabilities.can_read_session_metadata);
        assert!(!capabilities.can_read_logs);
        assert_eq!(capabilities.statuses[0].file_count, 1);
    }
}
