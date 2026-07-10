CREATE TABLE source_files (
    id INTEGER PRIMARY KEY,
    source_key TEXT NOT NULL UNIQUE,
    relative_path TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('session', 'archived_session', 'state_db', 'logs_db')),
    file_size INTEGER NOT NULL DEFAULT 0 CHECK (file_size >= 0),
    mtime_ns INTEGER NOT NULL DEFAULT 0 CHECK (mtime_ns >= 0),
    prefix_hash TEXT NOT NULL DEFAULT '',
    safe_offset INTEGER NOT NULL DEFAULT 0 CHECK (safe_offset >= 0),
    complete_line_offset INTEGER NOT NULL DEFAULT 0 CHECK (complete_line_offset >= 0),
    logs_rowid_watermark INTEGER NOT NULL DEFAULT 0 CHECK (logs_rowid_watermark >= 0),
    session_id TEXT,
    current_model_raw TEXT,
    current_pricing_model_id TEXT,
    model_provider TEXT,
    cumulative_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cumulative_input_tokens >= 0),
    cumulative_cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cumulative_cached_input_tokens >= 0),
    cumulative_output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cumulative_output_tokens >= 0),
    cumulative_reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cumulative_reasoning_tokens >= 0),
    relevant_event_ordinal INTEGER NOT NULL DEFAULT 0 CHECK (relevant_event_ordinal >= 0),
    open_task_turn_id TEXT,
    open_task_started_at_ms INTEGER,
    parser_version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'indexing', 'ready', 'stale', 'missing', 'error')),
    last_error_code TEXT,
    last_seen_at_ms INTEGER NOT NULL DEFAULT 0,
    UNIQUE (source_kind, relative_path)
) STRICT;

CREATE TABLE sync_runs (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode IN ('initial', 'incremental', 'rebuild', 'repair', 'retention', 'reprice')),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'cancelled', 'failed')),
    stage TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    files_total INTEGER NOT NULL DEFAULT 0 CHECK (files_total >= 0),
    files_completed INTEGER NOT NULL DEFAULT 0 CHECK (files_completed >= 0),
    bytes_total INTEGER NOT NULL DEFAULT 0 CHECK (bytes_total >= 0),
    bytes_read INTEGER NOT NULL DEFAULT 0 CHECK (bytes_read >= 0),
    events_written INTEGER NOT NULL DEFAULT 0 CHECK (events_written >= 0),
    records_skipped INTEGER NOT NULL DEFAULT 0 CHECK (records_skipped >= 0),
    parse_failures INTEGER NOT NULL DEFAULT 0 CHECK (parse_failures >= 0),
    error_code TEXT,
    elapsed_ms INTEGER,
    peak_memory_bytes INTEGER
) STRICT;

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    normalized_path TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    alias TEXT,
    ignored INTEGER NOT NULL DEFAULT 0 CHECK (ignored IN (0, 1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    source_file_id INTEGER REFERENCES source_files(id) ON DELETE SET NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    synthetic_title TEXT NOT NULL,
    started_at_ms INTEGER,
    ended_at_ms INTEGER,
    active_ms INTEGER NOT NULL DEFAULT 0 CHECK (active_ms >= 0),
    active_method TEXT NOT NULL DEFAULT 'unknown' CHECK (active_method IN ('lifecycle', 'idle_estimate', 'span_estimate', 'unknown')),
    active_is_estimate INTEGER NOT NULL DEFAULT 1 CHECK (active_is_estimate IN (0, 1)),
    model_provider TEXT NOT NULL DEFAULT 'unknown',
    latest_model_raw TEXT NOT NULL DEFAULT 'unknown',
    latest_pricing_model_id TEXT,
    primary_model_raw TEXT NOT NULL DEFAULT 'unknown',
    primary_pricing_model_id TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    fresh_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (fresh_input_tokens >= 0),
    cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cached_input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK (reasoning_tokens >= 0),
    total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    estimated_cost_microusd INTEGER,
    unpriced_event_count INTEGER NOT NULL DEFAULT 0 CHECK (unpriced_event_count >= 0),
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    integrity_status TEXT NOT NULL DEFAULT 'partial' CHECK (integrity_status IN ('complete', 'partial', 'warning', 'error')),
    warning_count INTEGER NOT NULL DEFAULT 0 CHECK (warning_count >= 0),
    parser_version INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE usage_events (
    id INTEGER PRIMARY KEY,
    event_key TEXT NOT NULL UNIQUE,
    source_file_id INTEGER NOT NULL REFERENCES source_files(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    byte_offset INTEGER NOT NULL CHECK (byte_offset >= 0),
    event_ordinal INTEGER NOT NULL CHECK (event_ordinal >= 0),
    occurred_at_ms INTEGER NOT NULL,
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    day_start_utc_ms INTEGER NOT NULL,
    day_end_utc_ms INTEGER NOT NULL,
    model_provider TEXT NOT NULL,
    model_raw TEXT NOT NULL,
    pricing_model_id TEXT,
    input_tokens INTEGER NOT NULL CHECK (input_tokens >= 0),
    fresh_input_tokens INTEGER NOT NULL CHECK (fresh_input_tokens >= 0),
    cached_input_tokens INTEGER NOT NULL CHECK (cached_input_tokens >= 0),
    output_tokens INTEGER NOT NULL CHECK (output_tokens >= 0),
    reasoning_tokens INTEGER NOT NULL CHECK (reasoning_tokens >= 0),
    total_tokens INTEGER NOT NULL CHECK (total_tokens >= 0),
    estimated_cost_microusd INTEGER,
    pricing_revision INTEGER,
    integrity_status TEXT NOT NULL DEFAULT 'complete' CHECK (integrity_status IN ('complete', 'partial', 'warning')),
    UNIQUE (source_file_id, byte_offset)
) STRICT;

CREATE TABLE session_model_segments (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    segment_index INTEGER NOT NULL CHECK (segment_index >= 0),
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER,
    model_provider TEXT NOT NULL,
    model_raw TEXT NOT NULL,
    pricing_model_id TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cached_input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK (reasoning_tokens >= 0),
    estimated_cost_microusd INTEGER,
    unpriced_event_count INTEGER NOT NULL DEFAULT 0 CHECK (unpriced_event_count >= 0),
    UNIQUE (session_id, segment_index)
) STRICT;

CREATE TABLE activity_segments (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    segment_index INTEGER NOT NULL CHECK (segment_index >= 0),
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER NOT NULL,
    active_ms INTEGER NOT NULL CHECK (active_ms >= 0),
    method TEXT NOT NULL CHECK (method IN ('lifecycle', 'idle_estimate', 'span_estimate')),
    is_estimate INTEGER NOT NULL CHECK (is_estimate IN (0, 1)),
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    model_provider TEXT NOT NULL,
    model_raw TEXT NOT NULL,
    pricing_model_id TEXT NOT NULL DEFAULT '',
    UNIQUE (session_id, segment_index)
) STRICT;

CREATE TABLE tool_events (
    id INTEGER PRIMARY KEY,
    event_key TEXT NOT NULL UNIQUE,
    source_file_id INTEGER NOT NULL REFERENCES source_files(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    byte_offset INTEGER NOT NULL CHECK (byte_offset >= 0),
    event_ordinal INTEGER NOT NULL CHECK (event_ordinal >= 0),
    sub_index INTEGER NOT NULL DEFAULT 0 CHECK (sub_index >= 0),
    occurred_at_ms INTEGER NOT NULL,
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    day_start_utc_ms INTEGER NOT NULL,
    day_end_utc_ms INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('search', 'read', 'write', 'edit', 'execute', 'other')),
    operation_kind TEXT NOT NULL CHECK (operation_kind IN ('read_only', 'mutating', 'mixed', 'unknown')),
    UNIQUE (source_file_id, byte_offset, sub_index)
) STRICT;

CREATE TABLE session_daily_usage (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    day_start_utc_ms INTEGER NOT NULL,
    day_end_utc_ms INTEGER NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    model_provider TEXT NOT NULL,
    model_raw TEXT NOT NULL,
    pricing_model_id TEXT NOT NULL DEFAULT '',
    archived INTEGER NOT NULL CHECK (archived IN (0, 1)),
    active_ms INTEGER NOT NULL DEFAULT 0 CHECK (active_ms >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    fresh_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (fresh_input_tokens >= 0),
    cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cached_input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK (reasoning_tokens >= 0),
    total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    priced_cost_microusd INTEGER NOT NULL DEFAULT 0 CHECK (priced_cost_microusd >= 0),
    priced_event_count INTEGER NOT NULL DEFAULT 0 CHECK (priced_event_count >= 0),
    unpriced_event_count INTEGER NOT NULL DEFAULT 0 CHECK (unpriced_event_count >= 0),
    last_activity_at_ms INTEGER,
    PRIMARY KEY (session_id, local_date, timezone_id, model_provider, model_raw, pricing_model_id, archived)
) STRICT, WITHOUT ROWID;

CREATE TABLE session_daily_tool (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    day_start_utc_ms INTEGER NOT NULL,
    day_end_utc_ms INTEGER NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    archived INTEGER NOT NULL CHECK (archived IN (0, 1)),
    tool_name TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('search', 'read', 'write', 'edit', 'execute', 'other')),
    operation_kind TEXT NOT NULL CHECK (operation_kind IN ('read_only', 'mutating', 'mixed', 'unknown')),
    call_count INTEGER NOT NULL DEFAULT 0 CHECK (call_count >= 0),
    last_activity_at_ms INTEGER,
    PRIMARY KEY (session_id, local_date, timezone_id, archived, tool_name, category, operation_kind)
) STRICT, WITHOUT ROWID;

CREATE TABLE daily_usage_rollups (
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    day_start_utc_ms INTEGER NOT NULL,
    day_end_utc_ms INTEGER NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    model_provider TEXT NOT NULL,
    model_raw TEXT NOT NULL,
    pricing_model_id TEXT NOT NULL DEFAULT '',
    archived INTEGER NOT NULL CHECK (archived IN (0, 1)),
    session_count INTEGER NOT NULL DEFAULT 0 CHECK (session_count >= 0),
    active_ms INTEGER NOT NULL DEFAULT 0 CHECK (active_ms >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    fresh_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (fresh_input_tokens >= 0),
    cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cached_input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK (reasoning_tokens >= 0),
    total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    priced_cost_microusd INTEGER NOT NULL DEFAULT 0 CHECK (priced_cost_microusd >= 0),
    unpriced_event_count INTEGER NOT NULL DEFAULT 0 CHECK (unpriced_event_count >= 0),
    last_activity_at_ms INTEGER,
    PRIMARY KEY (local_date, timezone_id, workspace_id, model_provider, model_raw, pricing_model_id, archived)
) STRICT, WITHOUT ROWID;

CREATE TABLE daily_tool_rollups (
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    day_start_utc_ms INTEGER NOT NULL,
    day_end_utc_ms INTEGER NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    archived INTEGER NOT NULL CHECK (archived IN (0, 1)),
    tool_name TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('search', 'read', 'write', 'edit', 'execute', 'other')),
    operation_kind TEXT NOT NULL CHECK (operation_kind IN ('read_only', 'mutating', 'mixed', 'unknown')),
    call_count INTEGER NOT NULL DEFAULT 0 CHECK (call_count >= 0),
    session_count INTEGER NOT NULL DEFAULT 0 CHECK (session_count >= 0),
    PRIMARY KEY (local_date, timezone_id, workspace_id, archived, tool_name, category, operation_kind)
) STRICT, WITHOUT ROWID;

CREATE TABLE model_prices (
    id INTEGER PRIMARY KEY,
    pricing_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    display_name TEXT NOT NULL,
    input_per_million_usd TEXT NOT NULL,
    output_per_million_usd TEXT NOT NULL,
    cache_read_per_million_usd TEXT NOT NULL,
    cache_write_per_million_usd TEXT,
    default_input_per_million_usd TEXT NOT NULL,
    default_output_per_million_usd TEXT NOT NULL,
    default_cache_read_per_million_usd TEXT NOT NULL,
    default_cache_write_per_million_usd TEXT,
    is_builtin INTEGER NOT NULL CHECK (is_builtin IN (0, 1)),
    is_overridden INTEGER NOT NULL DEFAULT 0 CHECK (is_overridden IN (0, 1)),
    is_deleted INTEGER NOT NULL DEFAULT 0 CHECK (is_deleted IN (0, 1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    source_url TEXT,
    source_updated_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE (provider, pricing_id)
) STRICT;

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_source_files_status ON source_files(status, source_kind);
CREATE INDEX idx_source_files_session ON source_files(session_id);
CREATE INDEX idx_sync_runs_started ON sync_runs(started_at_ms DESC);
CREATE INDEX idx_workspaces_activity ON workspaces(ignored, updated_at_ms DESC);
CREATE INDEX idx_sessions_workspace_time ON sessions(workspace_id, started_at_ms DESC, id);
CREATE INDEX idx_sessions_provider_model_time ON sessions(model_provider, latest_model_raw, started_at_ms DESC);
CREATE INDEX idx_sessions_archived_time ON sessions(archived, started_at_ms DESC);
CREATE INDEX idx_usage_events_time ON usage_events(occurred_at_ms DESC, id DESC);
CREATE INDEX idx_usage_events_session_time ON usage_events(session_id, occurred_at_ms, id);
CREATE INDEX idx_usage_events_filter_time ON usage_events(model_provider, model_raw, occurred_at_ms);
CREATE INDEX idx_model_segments_session ON session_model_segments(session_id, segment_index);
CREATE INDEX idx_activity_segments_session ON activity_segments(session_id, started_at_ms);
CREATE INDEX idx_tool_events_time ON tool_events(occurred_at_ms DESC, id DESC);
CREATE INDEX idx_tool_events_session ON tool_events(session_id, occurred_at_ms);
CREATE INDEX idx_tool_events_category_time ON tool_events(category, occurred_at_ms);
CREATE INDEX idx_session_daily_usage_date ON session_daily_usage(local_date, timezone_id);
CREATE INDEX idx_session_daily_usage_filter ON session_daily_usage(workspace_id, model_provider, model_raw, local_date);
CREATE INDEX idx_session_daily_tool_date ON session_daily_tool(local_date, timezone_id);
CREATE INDEX idx_daily_usage_filter ON daily_usage_rollups(workspace_id, model_provider, model_raw, local_date);
CREATE INDEX idx_daily_usage_date ON daily_usage_rollups(local_date, timezone_id);
CREATE INDEX idx_daily_tool_filter ON daily_tool_rollups(workspace_id, category, local_date);
CREATE INDEX idx_model_prices_lookup ON model_prices(provider, pricing_id, is_deleted);
