use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::DateTime;
use serde_json::Value;

use super::{
    ActivitySegment, ChangeAction, ModelChanged, ParsedRecord, SessionMetadata, SourceChange,
    SourceCursor, StreamOutcome, ToolEvent, UsageEvent,
};
use crate::activity::{ToolInvocation, classify};
use crate::error::{AppError, AppResult};
use crate::pricing::{canonical_model_provider, normalize_model_id};

pub(super) fn stream_jsonl(
    change: &SourceChange,
    sink: &mut dyn FnMut(ParsedRecord) -> AppResult<()>,
    cancel: &AtomicBool,
) -> AppResult<StreamOutcome> {
    let mut cursor = match change.action {
        ChangeAction::Replay => SourceCursor::default(),
        ChangeAction::Append => change.cursor.clone(),
        ChangeAction::MetadataOnly | ChangeAction::Missing => return Err(AppError::filesystem()),
    };
    let start_offset = cursor.safe_offset;
    let mut file = File::open(&change.path)?;
    file.seek(SeekFrom::Start(start_offset))?;
    let mut reader = BufReader::with_capacity(128 * 1024, file);
    let mut line = Vec::with_capacity(16 * 1024);
    let mut records_emitted = 0_u64;
    let mut parse_failures = 0_u64;
    let mut bytes_read = 0_u64;
    let mut incomplete_tail = false;
    let mut cancelled = false;

    loop {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        line.clear();
        let line_start = cursor.safe_offset;
        let length = reader.read_until(b'\n', &mut line)?;
        if length == 0 {
            break;
        }
        bytes_read = bytes_read.saturating_add(length as u64);
        if line.last() != Some(&b'\n') {
            incomplete_tail = true;
            break;
        }

        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let next_offset = line_start.saturating_add(length as u64);
        if !line.iter().all(u8::is_ascii_whitespace) {
            match serde_json::from_slice::<Value>(&line) {
                Ok(value) => {
                    for record in parse_value(&value, line_start, &mut cursor) {
                        sink(record)?;
                        records_emitted = records_emitted.saturating_add(1);
                    }
                }
                Err(_) => {
                    parse_failures = parse_failures.saturating_add(1);
                }
            }
        }
        // 只有完整换行才推进安全偏移；不完整尾行会在下次追加后重新读取。
        cursor.safe_offset = next_offset;
        cursor.complete_line_offset = next_offset;
    }

    Ok(StreamOutcome {
        cursor,
        bytes_read,
        records_emitted,
        parse_failures,
        incomplete_tail,
        cancelled,
    })
}

fn parse_value(value: &Value, byte_offset: u64, cursor: &mut SourceCursor) -> Vec<ParsedRecord> {
    let Some(event_type) = value.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Vec::new();
    };
    let occurred_at_ms = parse_timestamp(value.get("timestamp"));

    match event_type {
        "session_meta" => parse_session_metadata(payload, occurred_at_ms, byte_offset, cursor),
        "turn_context" => parse_turn_context(payload, occurred_at_ms, byte_offset, cursor),
        "event_msg" => parse_event_message(payload, occurred_at_ms, byte_offset, cursor),
        "response_item" => parse_tool_call(payload, occurred_at_ms, byte_offset, cursor),
        _ => Vec::new(),
    }
}

fn parse_session_metadata(
    payload: &serde_json::Map<String, Value>,
    outer_timestamp: Option<i64>,
    byte_offset: u64,
    cursor: &mut SourceCursor,
) -> Vec<ParsedRecord> {
    let Some(session_id) = ["session_id", "sessionId", "id"]
        .iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
    else {
        return Vec::new();
    };
    let provider = payload
        .get("model_provider")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(canonical_model_provider);
    if let Some(provider) = &provider {
        cursor.model_provider = Some(provider.clone());
    }
    let legacy_model = payload
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let occurred_at_ms = payload
        .get("timestamp")
        .and_then(|value| parse_timestamp(Some(value)))
        .or(outer_timestamp);
    let ordinal = next_ordinal(cursor);
    let mut records = vec![ParsedRecord::SessionMetadata(SessionMetadata {
        event_ordinal: ordinal,
        occurred_at_ms,
        session_id: session_id.to_string(),
        cwd: payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string),
        model_provider: provider,
        legacy_model: legacy_model.clone(),
    })];

    if let Some(model) = legacy_model {
        records.push(ParsedRecord::ModelChanged(set_current_model(
            model,
            occurred_at_ms,
            byte_offset,
            ordinal,
            cursor,
        )));
    }
    records
}

fn parse_turn_context(
    payload: &serde_json::Map<String, Value>,
    occurred_at_ms: Option<i64>,
    byte_offset: u64,
    cursor: &mut SourceCursor,
) -> Vec<ParsedRecord> {
    let model = payload
        .get("model")
        .or_else(|| payload.get("info").and_then(|info| info.get("model")))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let Some(model) = model else {
        return Vec::new();
    };
    if cursor.current_model_raw.as_deref() == Some(model) {
        return Vec::new();
    }
    let ordinal = next_ordinal(cursor);
    vec![ParsedRecord::ModelChanged(set_current_model(
        model.to_string(),
        occurred_at_ms,
        byte_offset,
        ordinal,
        cursor,
    ))]
}

fn parse_event_message(
    payload: &serde_json::Map<String, Value>,
    occurred_at_ms: Option<i64>,
    byte_offset: u64,
    cursor: &mut SourceCursor,
) -> Vec<ParsedRecord> {
    match payload.get("type").and_then(Value::as_str) {
        Some("token_count") => parse_token_count(payload, occurred_at_ms, byte_offset, cursor),
        Some("task_started") => {
            cursor.open_task_turn_id = payload
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            cursor.open_task_started_at_ms = payload
                .get("started_at")
                .and_then(|value| parse_timestamp(Some(value)))
                .or(occurred_at_ms);
            Vec::new()
        }
        Some("task_complete" | "task_completed" | "turn_aborted") => {
            parse_task_end(payload, occurred_at_ms, byte_offset, cursor)
        }
        _ => Vec::new(),
    }
}

fn parse_token_count(
    payload: &serde_json::Map<String, Value>,
    occurred_at_ms: Option<i64>,
    byte_offset: u64,
    cursor: &mut SourceCursor,
) -> Vec<ParsedRecord> {
    let Some(info) = payload.get("info").and_then(Value::as_object) else {
        return Vec::new();
    };
    if let Some(model) = info
        .get("model")
        .or_else(|| info.get("model_name"))
        .or_else(|| payload.get("model"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        && cursor.current_model_raw.as_deref() != Some(model)
    {
        let normalized = normalize_model_id(model);
        cursor.current_model_raw = Some(model.to_string());
        cursor.current_pricing_model_id = Some(normalized.exact);
    }

    let (usage, cumulative) =
        if let Some(total) = info.get("total_token_usage").and_then(parse_token_values) {
            (delta_from_cursor(&total, cursor), Some(total))
        } else if let Some(last) = info.get("last_token_usage").and_then(parse_token_values) {
            (last, None)
        } else {
            return Vec::new();
        };
    if let Some(total) = cumulative {
        cursor.cumulative_input_tokens = total.input;
        cursor.cumulative_cached_input_tokens = total.cached;
        cursor.cumulative_output_tokens = total.output;
        cursor.cumulative_reasoning_tokens = total.reasoning;
    }

    let cached = usage.cached.min(usage.input);
    let reasoning = usage.reasoning.min(usage.output);
    let fresh = usage.input.saturating_sub(cached);
    if usage.input == 0 && usage.output == 0 && reasoning == 0 {
        return Vec::new();
    }
    let Some(occurred_at_ms) = occurred_at_ms else {
        return Vec::new();
    };
    let ordinal = next_ordinal(cursor);
    let model_raw = cursor
        .current_model_raw
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let pricing_model_id = cursor
        .current_pricing_model_id
        .clone()
        .unwrap_or_else(|| normalize_model_id(&model_raw).exact);
    vec![ParsedRecord::Usage(UsageEvent {
        byte_offset,
        event_ordinal: ordinal,
        occurred_at_ms,
        model_provider: cursor
            .model_provider
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        model_raw,
        pricing_model_id,
        input_tokens: usage.input,
        fresh_input_tokens: fresh,
        cached_input_tokens: cached,
        output_tokens: usage.output,
        reasoning_tokens: reasoning,
        total_tokens: usage.input.saturating_add(usage.output),
    })]
}

fn parse_task_end(
    payload: &serde_json::Map<String, Value>,
    occurred_at_ms: Option<i64>,
    byte_offset: u64,
    cursor: &mut SourceCursor,
) -> Vec<ParsedRecord> {
    let ended_at_ms = payload
        .get("completed_at")
        .and_then(|value| parse_timestamp(Some(value)))
        .or(occurred_at_ms);
    let Some(ended_at_ms) = ended_at_ms else {
        return Vec::new();
    };
    let duration_ms = payload.get("duration_ms").and_then(Value::as_u64);
    let turn_id = payload.get("turn_id").and_then(Value::as_str);
    let matching_open = cursor.open_task_turn_id.as_deref().is_none()
        || turn_id.is_none()
        || cursor.open_task_turn_id.as_deref() == turn_id;
    let started_at_ms = if matching_open {
        cursor.open_task_started_at_ms
    } else {
        None
    }
    .or_else(|| {
        duration_ms.and_then(|duration| ended_at_ms.checked_sub(i64::try_from(duration).ok()?))
    });
    let Some(started_at_ms) = started_at_ms else {
        return Vec::new();
    };
    if ended_at_ms < started_at_ms {
        return Vec::new();
    }
    let active_ms = duration_ms.unwrap_or_else(|| (ended_at_ms - started_at_ms) as u64);
    if matching_open {
        cursor.open_task_turn_id = None;
        cursor.open_task_started_at_ms = None;
    }
    let ordinal = next_ordinal(cursor);
    vec![ParsedRecord::Activity(ActivitySegment {
        byte_offset,
        event_ordinal: ordinal,
        started_at_ms,
        ended_at_ms,
        active_ms,
    })]
}

fn parse_tool_call(
    payload: &serde_json::Map<String, Value>,
    occurred_at_ms: Option<i64>,
    byte_offset: u64,
    cursor: &mut SourceCursor,
) -> Vec<ParsedRecord> {
    let payload_type = payload.get("type").and_then(Value::as_str);
    if !matches!(payload_type, Some("custom_tool_call" | "function_call")) {
        return Vec::new();
    }
    let Some(occurred_at_ms) = occurred_at_ms else {
        return Vec::new();
    };
    let Some(tool_name) = payload.get("name").and_then(Value::as_str) else {
        return Vec::new();
    };
    let raw_arguments = match payload_type {
        Some("custom_tool_call") => payload.get("input").and_then(Value::as_str),
        Some("function_call") => payload.get("arguments").and_then(Value::as_str),
        _ => None,
    };
    let classifications = classify(ToolInvocation {
        tool_name,
        raw_arguments,
    });
    if classifications.is_empty() {
        return Vec::new();
    }
    let ordinal = next_ordinal(cursor);
    classifications
        .into_iter()
        .enumerate()
        .map(|(index, tool)| {
            ParsedRecord::Tool(ToolEvent {
                byte_offset,
                event_ordinal: ordinal,
                sub_index: index as u32,
                occurred_at_ms,
                tool_name: tool.tool_name,
                category: tool.category,
                operation_kind: tool.operation_kind,
            })
        })
        .collect()
}

fn set_current_model(
    model_raw: String,
    occurred_at_ms: Option<i64>,
    byte_offset: u64,
    event_ordinal: u64,
    cursor: &mut SourceCursor,
) -> ModelChanged {
    let pricing_model_id = normalize_model_id(&model_raw).exact;
    cursor.current_model_raw = Some(model_raw.clone());
    cursor.current_pricing_model_id = Some(pricing_model_id.clone());
    ModelChanged {
        byte_offset,
        event_ordinal,
        occurred_at_ms,
        model_provider: cursor
            .model_provider
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        model_raw,
        pricing_model_id,
    }
}

#[derive(Debug, Clone, Copy)]
struct TokenValues {
    input: u64,
    cached: u64,
    output: u64,
    reasoning: u64,
}

fn parse_token_values(value: &Value) -> Option<TokenValues> {
    let object = value.as_object()?;
    Some(TokenValues {
        input: object
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached: object
            .get("cached_input_tokens")
            .or_else(|| object.get("cache_read_input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output: object
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reasoning: object
            .get("reasoning_output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

fn delta_from_cursor(current: &TokenValues, cursor: &SourceCursor) -> TokenValues {
    TokenValues {
        input: current.input.saturating_sub(cursor.cumulative_input_tokens),
        cached: current
            .cached
            .saturating_sub(cursor.cumulative_cached_input_tokens),
        output: current
            .output
            .saturating_sub(cursor.cumulative_output_tokens),
        reasoning: current
            .reasoning
            .saturating_sub(cursor.cumulative_reasoning_tokens),
    }
}

fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|timestamp| timestamp.timestamp_millis());
    }
    let numeric = value.as_i64()?;
    Some(if numeric.abs() < 100_000_000_000 {
        numeric.saturating_mul(1000)
    } else {
        numeric
    })
}

fn next_ordinal(cursor: &mut SourceCursor) -> u64 {
    cursor.relevant_event_ordinal = cursor.relevant_event_ordinal.saturating_add(1);
    cursor.relevant_event_ordinal
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::json;

    use super::*;
    use crate::source::{SourceKind, prefix_hash};

    fn line(value: &Value) -> String {
        serde_json::to_string(&value).unwrap()
    }

    fn change(path: &std::path::Path, action: ChangeAction, cursor: SourceCursor) -> SourceChange {
        let metadata = std::fs::metadata(path).unwrap();
        SourceChange {
            source_key: "jsonl:test".to_string(),
            relative_path: "sessions/test.jsonl".to_string(),
            path: path.to_path_buf(),
            kind: SourceKind::Session,
            session_id: None,
            action,
            file_size: metadata.len(),
            mtime_ns: 1,
            prefix_hash: prefix_hash(path).unwrap(),
            cursor,
        }
    }

    fn run(
        path: &std::path::Path,
        action: ChangeAction,
        cursor: SourceCursor,
    ) -> (Vec<ParsedRecord>, StreamOutcome) {
        let mut records = Vec::new();
        let outcome = stream_jsonl(
            &change(path, action, cursor),
            &mut |record| {
                records.push(record);
                Ok(())
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        (records, outcome)
    }

    #[test]
    fn parses_current_model_provider_switches_and_token_deltas() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.jsonl");
        let values = [
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"session_meta","payload":{"id":"thread-1","cwd":"C:\\work","model_provider":"openai"}}),
            json!({"timestamp":"2026-07-10T01:00:01Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}),
            json!({"timestamp":"2026-07-10T01:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20,"reasoning_output_tokens":5}}}}),
            json!({"timestamp":"2026-07-10T01:00:03Z","type":"turn_context","payload":{"model":"gpt-5.6-terra"}}),
            json!({"timestamp":"2026-07-10T01:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":180,"cached_input_tokens":60,"output_tokens":50,"reasoning_output_tokens":10}}}}),
        ];
        std::fs::write(
            &path,
            values.iter().map(line).collect::<Vec<_>>().join("\n") + "\n",
        )
        .unwrap();

        let (records, outcome) = run(&path, ChangeAction::Replay, SourceCursor::default());
        let usage: Vec<_> = records
            .iter()
            .filter_map(|record| match record {
                ParsedRecord::Usage(event) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].model_raw, "gpt-5.6-sol");
        assert_eq!(usage[0].model_provider, "openai");
        assert_eq!(usage[0].fresh_input_tokens, 60);
        assert_eq!(usage[0].reasoning_tokens, 5);
        assert_eq!(usage[1].model_raw, "gpt-5.6-terra");
        assert_eq!(usage[1].input_tokens, 80);
        assert_eq!(usage[1].cached_input_tokens, 20);
        assert_eq!(usage[1].output_tokens, 30);
        assert_eq!(usage[1].reasoning_tokens, 5);
        assert_eq!(
            outcome.cursor.current_model_raw.as_deref(),
            Some("gpt-5.6-terra")
        );
    }

    #[test]
    fn supports_legacy_session_model_and_cached_compatibility_field() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.jsonl");
        let content = [
            json!({"timestamp":"2025-01-01T00:00:00Z","type":"session_meta","payload":{"id":"legacy","model":"gpt-5.1","model_provider":"openai"}}),
            json!({"timestamp":"2025-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"cache_read_input_tokens":20,"output_tokens":10,"reasoning_output_tokens":3}}}}),
        ]
        .iter()
        .map(line)
        .collect::<Vec<_>>()
        .join("\n") + "\n";
        std::fs::write(&path, content).unwrap();

        let (records, _) = run(&path, ChangeAction::Replay, SourceCursor::default());
        let usage = records
            .iter()
            .find_map(|record| match record {
                ParsedRecord::Usage(event) => Some(event),
                _ => None,
            })
            .unwrap();
        assert_eq!(usage.model_raw, "gpt-5.1");
        assert_eq!(usage.cached_input_tokens, 20);
        assert_eq!(usage.fresh_input_tokens, 30);
        assert_eq!(usage.reasoning_tokens, 3);
    }

    #[test]
    fn resumes_from_last_complete_newline_after_incomplete_tail() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tail.jsonl");
        let metadata = line(
            &json!({"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":"tail","model":"gpt-5.6","model_provider":"openai"}}),
        );
        let token = line(
            &json!({"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}),
        );
        std::fs::write(&path, format!("{metadata}\n{token}")).unwrap();

        let (first_records, first) = run(&path, ChangeAction::Replay, SourceCursor::default());
        assert!(first.incomplete_tail);
        assert!(
            !first_records
                .iter()
                .any(|record| matches!(record, ParsedRecord::Usage(_)))
        );
        assert_eq!(first.cursor.safe_offset, (metadata.len() + 1) as u64);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file).unwrap();
        let (second_records, second) = run(&path, ChangeAction::Append, first.cursor);
        assert!(!second.incomplete_tail);
        assert_eq!(
            second_records
                .iter()
                .filter(|record| matches!(record, ParsedRecord::Usage(_)))
                .count(),
            1
        );
    }

    #[test]
    fn lifecycle_events_produce_non_estimated_activity_segments() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("activity.jsonl");
        let content = [
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","started_at":"2026-07-10T01:00:00Z"}}),
            json!({"timestamp":"2026-07-10T01:00:07Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","completed_at":"2026-07-10T01:00:07Z","duration_ms":7000}}),
        ]
        .iter()
        .map(line)
        .collect::<Vec<_>>()
        .join("\n") + "\n";
        std::fs::write(&path, content).unwrap();

        let (records, _) = run(&path, ChangeAction::Replay, SourceCursor::default());
        let segment = records
            .iter()
            .find_map(|record| match record {
                ParsedRecord::Activity(segment) => Some(segment),
                _ => None,
            })
            .unwrap();
        assert_eq!(segment.active_ms, 7000);
        assert_eq!(segment.ended_at_ms - segment.started_at_ms, 7000);
    }

    #[test]
    fn tool_arguments_never_cross_the_source_interface() {
        const SENSITIVE: &str = "UNIQUE_PRIVATE_ARGUMENT_C:\\top-secret\\code.rs";
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tools.jsonl");
        let input = format!(
            "await tools.shell_command({{command: 'Get-Content {SENSITIVE}'}}); await tools.apply_patch('{SENSITIVE}');"
        );
        std::fs::write(
            &path,
            line(&json!({"timestamp":"2026-07-10T01:00:00Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":input}})) + "\n",
        )
        .unwrap();

        let (records, _) = run(&path, ChangeAction::Replay, SourceCursor::default());
        let tools: Vec<_> = records
            .iter()
            .filter_map(|record| match record {
                ParsedRecord::Tool(tool) => Some(tool),
                _ => None,
            })
            .collect();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].tool_name, "shell_command");
        assert_eq!(tools[1].tool_name, "apply_patch");
        let debug = format!("{records:?}");
        assert!(!debug.contains(SENSITIVE));
        assert!(!debug.contains("top-secret"));
    }
}
