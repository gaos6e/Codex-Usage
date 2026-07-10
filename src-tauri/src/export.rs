use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::query::{
    DashboardSnapshot, ListQuery, ModelRow, Page, SessionRow, ToolsSnapshot, UsageFilters,
    UsageQuery, WorkspaceRow,
};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportScope {
    Dashboard,
    Workspaces,
    Sessions,
    Models,
    Tools,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPrivacy {
    Anonymous,
    FullPath,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub scope: ExportScope,
    pub privacy: ExportPrivacy,
    pub filters: UsageFilters,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub bytes_written: u64,
    pub rows_written: usize,
}

pub struct UsageExporter {
    query: Arc<UsageQuery>,
}

impl UsageExporter {
    pub fn new(query: Arc<UsageQuery>) -> Self {
        Self { query }
    }

    pub fn export_to_path(&self, request: &ExportRequest, path: &str) -> AppResult<ExportResult> {
        let output = self.build(request)?;
        std::fs::write(path, output.content.as_bytes())?;
        Ok(ExportResult {
            path: path.to_string(),
            bytes_written: output.content.len() as u64,
            rows_written: output.rows,
        })
    }

    fn build(&self, request: &ExportRequest) -> AppResult<ExportOutput> {
        match request.scope {
            ExportScope::Dashboard => {
                let mut snapshot = self.query.dashboard(&request.filters)?;
                if matches!(request.privacy, ExportPrivacy::Anonymous) {
                    for (index, option) in snapshot.filter_options.workspaces.iter_mut().enumerate()
                    {
                        option.label = format!("Workspace {}", index + 1);
                    }
                }
                export_dashboard(request.format, snapshot)
            }
            ExportScope::Workspaces => {
                let mut rows = self.collect_workspaces(&request.filters)?;
                if matches!(request.privacy, ExportPrivacy::Anonymous) {
                    anonymize_workspaces(&mut rows);
                }
                export_workspaces(request.format, rows)
            }
            ExportScope::Sessions => {
                let mut rows = self.collect_sessions(&request.filters)?;
                if matches!(request.privacy, ExportPrivacy::Anonymous) {
                    anonymize_sessions(&mut rows);
                }
                export_sessions(request.format, rows)
            }
            ExportScope::Models => {
                export_models(request.format, self.collect_models(&request.filters)?)
            }
            ExportScope::Tools => export_tools(request.format, self.query.tools(&request.filters)?),
        }
    }

    fn collect_workspaces(&self, filters: &UsageFilters) -> AppResult<Vec<WorkspaceRow>> {
        collect_pages(|page| self.query.workspaces(&list_query(filters, page, "recent")))
    }

    fn collect_sessions(&self, filters: &UsageFilters) -> AppResult<Vec<SessionRow>> {
        collect_pages(|page| self.query.sessions(&list_query(filters, page, "recent")))
    }

    fn collect_models(&self, filters: &UsageFilters) -> AppResult<Vec<ModelRow>> {
        collect_pages(|page| self.query.models(&list_query(filters, page, "tokens")))
    }
}

fn list_query(filters: &UsageFilters, page: u32, sort: &str) -> ListQuery {
    ListQuery {
        filters: filters.clone(),
        workspace_ids: None,
        search: String::new(),
        sort: sort.into(),
        descending: true,
        page,
        page_size: 100,
    }
}

struct ExportOutput {
    content: String,
    rows: usize,
}

fn collect_pages<T>(mut query: impl FnMut(u32) -> AppResult<Page<T>>) -> AppResult<Vec<T>> {
    let mut page = 0_u32;
    let mut items = Vec::new();
    loop {
        let result = query(page)?;
        let total = result.total as usize;
        let page_empty = result.items.is_empty();
        items.extend(result.items);
        if page_empty || items.len() >= total {
            return Ok(items);
        }
        page = page.saturating_add(1);
        if page > 100_000 {
            return Err(AppError::new("export_too_large", "导出页数超出安全上限"));
        }
    }
}

fn export_dashboard(format: ExportFormat, snapshot: DashboardSnapshot) -> AppResult<ExportOutput> {
    if matches!(format, ExportFormat::Json) {
        let rows = snapshot.trend.len();
        return json_output(&snapshot, rows);
    }
    let mut csv = String::from("metric,value\r\n");
    let hero = snapshot.hero;
    for (metric, value) in [
        ("real_total_tokens", hero.real_total_tokens.to_string()),
        ("fresh_input_tokens", hero.fresh_input_tokens.to_string()),
        ("cached_input_tokens", hero.cached_input_tokens.to_string()),
        ("output_tokens", hero.output_tokens.to_string()),
        ("reasoning_tokens", hero.reasoning_tokens.to_string()),
        (
            "estimated_cost_microusd",
            optional_i64(hero.estimated_cost_microusd),
        ),
        ("session_count", hero.session_count.to_string()),
        ("active_ms", hero.active_ms.to_string()),
        ("active_days", hero.active_days.to_string()),
    ] {
        push_csv_row(&mut csv, &[metric, &value]);
    }
    Ok(ExportOutput {
        content: csv,
        rows: 9,
    })
}

fn export_workspaces(format: ExportFormat, rows: Vec<WorkspaceRow>) -> AppResult<ExportOutput> {
    if matches!(format, ExportFormat::Json) {
        return json_output_with_count(rows);
    }
    let mut csv = String::from(
        "workspace,path,sessions,total_tokens,cost_microusd,active_ms,active_days,last_activity_ms\r\n",
    );
    for row in &rows {
        push_csv_row(
            &mut csv,
            &[
                &row.label,
                &row.normalized_path,
                &row.session_count.to_string(),
                &row.total_tokens.to_string(),
                &optional_i64(row.estimated_cost_microusd),
                &row.active_ms.to_string(),
                &row.active_days.to_string(),
                &optional_i64(row.last_activity_at_ms),
            ],
        );
    }
    Ok(ExportOutput {
        content: csv,
        rows: rows.len(),
    })
}

fn export_sessions(format: ExportFormat, rows: Vec<SessionRow>) -> AppResult<ExportOutput> {
    if matches!(format, ExportFormat::Json) {
        return json_output_with_count(rows);
    }
    let mut csv = String::from(
        "session,workspace,started_ms,ended_ms,active_ms,active_method,provider,model,input_tokens,cached_tokens,output_tokens,reasoning_tokens,total_tokens,cost_microusd,archived,integrity\r\n",
    );
    for row in &rows {
        push_csv_row(
            &mut csv,
            &[
                &row.title,
                &row.workspace_label,
                &optional_i64(row.started_at_ms),
                &optional_i64(row.ended_at_ms),
                &row.active_ms.to_string(),
                &row.active_method,
                &row.model_provider,
                &row.latest_model,
                &row.input_tokens.to_string(),
                &row.cached_input_tokens.to_string(),
                &row.output_tokens.to_string(),
                &row.reasoning_tokens.to_string(),
                &row.total_tokens.to_string(),
                &optional_i64(row.estimated_cost_microusd),
                &row.archived.to_string(),
                &row.integrity_status,
            ],
        );
    }
    Ok(ExportOutput {
        content: csv,
        rows: rows.len(),
    })
}

fn export_models(format: ExportFormat, rows: Vec<ModelRow>) -> AppResult<ExportOutput> {
    if matches!(format, ExportFormat::Json) {
        return json_output_with_count(rows);
    }
    let mut csv = String::from(
        "provider,model,pricing_id,sessions,input_tokens,cached_tokens,output_tokens,reasoning_tokens,total_tokens,cost_microusd,unpriced_events,last_used_ms\r\n",
    );
    for row in &rows {
        push_csv_row(
            &mut csv,
            &[
                &row.provider,
                &row.model,
                row.pricing_model_id.as_deref().unwrap_or(""),
                &row.session_count.to_string(),
                &row.input_tokens.to_string(),
                &row.cached_input_tokens.to_string(),
                &row.output_tokens.to_string(),
                &row.reasoning_tokens.to_string(),
                &row.total_tokens.to_string(),
                &optional_i64(row.estimated_cost_microusd),
                &row.unpriced_event_count.to_string(),
                &optional_i64(row.last_used_at_ms),
            ],
        );
    }
    Ok(ExportOutput {
        content: csv,
        rows: rows.len(),
    })
}

fn export_tools(format: ExportFormat, snapshot: ToolsSnapshot) -> AppResult<ExportOutput> {
    if matches!(format, ExportFormat::Json) {
        let rows = snapshot.top_tools.len();
        return json_output(&snapshot, rows);
    }
    let mut csv = String::from("tool,category,operation_kind,calls,sessions\r\n");
    for row in &snapshot.top_tools {
        push_csv_row(
            &mut csv,
            &[
                &row.tool_name,
                &row.category,
                &row.operation_kind,
                &row.call_count.to_string(),
                &row.session_count.to_string(),
            ],
        );
    }
    Ok(ExportOutput {
        content: csv,
        rows: snapshot.top_tools.len(),
    })
}

fn json_output_with_count<T: Serialize>(value: Vec<T>) -> AppResult<ExportOutput> {
    let rows = value.len();
    json_output(&value, rows)
}

fn json_output(value: &impl Serialize, rows: usize) -> AppResult<ExportOutput> {
    let envelope = json!({
        "exportedAtMs": Utc::now().timestamp_millis(),
        "privacyNotice": "No conversation content or tool arguments are included.",
        "data": value,
    });
    Ok(ExportOutput {
        content: serde_json::to_string_pretty(&envelope)
            .map_err(|_| AppError::new("export_serialization_failed", "导出序列化失败"))?,
        rows,
    })
}

fn anonymize_workspaces(rows: &mut [WorkspaceRow]) {
    for (index, row) in rows.iter_mut().enumerate() {
        row.label = format!("Workspace {}", index + 1);
        row.normalized_path = "<anonymous>".to_string();
    }
}

fn anonymize_sessions(rows: &mut [SessionRow]) {
    let mut workspaces = HashMap::<String, usize>::new();
    let mut next = 1_usize;
    for row in rows {
        let index = *workspaces
            .entry(row.workspace_id.clone())
            .or_insert_with(|| {
                let value = next;
                next += 1;
                value
            });
        row.workspace_label = format!("Workspace {index}");
    }
}

fn push_csv_row(output: &mut String, fields: &[&str]) {
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&csv_field(field));
    }
    output.push_str("\r\n");
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn optional_i64(value: Option<i64>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escaping_preserves_columns() {
        assert_eq!(csv_field("repo,one"), "\"repo,one\"");
        assert_eq!(csv_field("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn anonymous_workspace_export_removes_names_and_paths() {
        let mut rows = vec![WorkspaceRow {
            id: "w1".into(),
            label: "secret-repo".into(),
            normalized_path: "C:/secret/repo".into(),
            ignored: false,
            session_count: 1,
            total_tokens: 2,
            estimated_cost_microusd: None,
            unpriced_event_count: 1,
            active_ms: 3,
            active_days: 1,
            last_activity_at_ms: None,
        }];
        anonymize_workspaces(&mut rows);
        let serialized = serde_json::to_string(&rows).unwrap();
        assert!(!serialized.contains("secret"));
        assert_eq!(rows[0].normalized_path, "<anonymous>");
    }
}
