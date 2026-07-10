use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

static NESTED_TOOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"tools\.([A-Za-z0-9_]+)\s*\(").expect("nested tool regular expression")
});
static SAFE_TOOL_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^A-Za-z0-9_.:\-]").expect("tool name regular expression"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Search,
    Read,
    Write,
    Edit,
    Execute,
    Other,
}

impl ToolCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Read => "read",
            Self::Write => "write",
            Self::Edit => "edit",
            Self::Execute => "execute",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    ReadOnly,
    Mutating,
    Mixed,
    Unknown,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Mutating => "mutating",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
        }
    }
}

/// 原始参数仅以借用形式进入 Activity，实现不会把它复制到结果或错误中。
pub struct ToolInvocation<'a> {
    pub tool_name: &'a str,
    pub raw_arguments: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedTool {
    pub tool_name: String,
    pub category: ToolCategory,
    pub operation_kind: OperationKind,
}

/// 将一次外层调用展开成实际工具计数。例如新版 `exec` 包装器中的多个
/// `tools.shell_command(...)` 会成为多条脱敏分类，但 JS、命令和路径不会返回。
pub fn classify(invocation: ToolInvocation<'_>) -> Vec<ClassifiedTool> {
    let outer = invocation.tool_name.trim();
    if matches!(
        outer.to_ascii_lowercase().as_str(),
        "exec" | "functions.exec"
    ) {
        if let Some(arguments) = invocation.raw_arguments {
            let nested: Vec<_> = NESTED_TOOL
                .captures_iter(arguments)
                .filter_map(|captures| captures.get(1).map(|name| name.as_str()))
                .map(|name| classify_one(name, Some(arguments)))
                .collect();
            if !nested.is_empty() {
                return nested;
            }
        }
    }

    vec![classify_one(outer, invocation.raw_arguments)]
}

fn classify_one(tool_name: &str, raw_arguments: Option<&str>) -> ClassifiedTool {
    let safe_name = sanitize_tool_name(tool_name);
    let folded = safe_name.to_ascii_lowercase();
    let (category, operation_kind) = if is_search_tool(&folded) {
        (ToolCategory::Search, OperationKind::ReadOnly)
    } else if is_read_tool(&folded) {
        (ToolCategory::Read, OperationKind::ReadOnly)
    } else if is_edit_tool(&folded) {
        (ToolCategory::Edit, OperationKind::Mutating)
    } else if is_write_tool(&folded) {
        (ToolCategory::Write, OperationKind::Mutating)
    } else if is_shell_tool(&folded) {
        (
            ToolCategory::Execute,
            classify_shell_operation(raw_arguments.unwrap_or_default()),
        )
    } else {
        (ToolCategory::Other, OperationKind::Unknown)
    };

    ClassifiedTool {
        tool_name: safe_name.into_owned(),
        category,
        operation_kind,
    }
}

fn sanitize_tool_name(raw: &str) -> Cow<'_, str> {
    let trimmed = raw.trim();
    let limited = if trimmed.len() > 96 {
        let boundary = trimmed
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= 96)
            .last()
            .unwrap_or(0);
        &trimmed[..boundary]
    } else {
        trimmed
    };
    SAFE_TOOL_NAME.replace_all(limited, "_")
}

fn is_search_tool(name: &str) -> bool {
    name.contains("search")
        || name == "rg"
        || name.ends_with("_find")
        || name.starts_with("find_")
        || name.contains("select_string")
}

fn is_read_tool(name: &str) -> bool {
    name.contains("read")
        || name.contains("view_image")
        || name.contains("screenshot")
        || name.starts_with("get_")
        || name.starts_with("list_")
        || name == "open"
}

fn is_edit_tool(name: &str) -> bool {
    name.contains("apply_patch")
        || name.contains("edit_file")
        || name.contains("replace_in_file")
        || name.contains("multi_edit")
}

fn is_write_tool(name: &str) -> bool {
    name.contains("write_file")
        || name.contains("create_file")
        || name.contains("save_file")
        || name.contains("delete_file")
}

fn is_shell_tool(name: &str) -> bool {
    name.contains("shell_command")
        || name.contains("exec_command")
        || matches!(name, "bash" | "shell" | "powershell" | "terminal")
}

fn classify_shell_operation(raw: &str) -> OperationKind {
    if raw.trim().is_empty() {
        return OperationKind::Unknown;
    }
    let folded = raw.to_ascii_lowercase();
    let mutating = [
        "remove-item",
        "move-item",
        "copy-item",
        "set-content",
        "add-content",
        "new-item",
        "apply_patch",
        "git commit",
        "git reset",
        "git checkout",
        "git clean",
        "npm install",
        "cargo fmt",
        "cargo fix",
        "mkdir",
        " rmdir",
        " del ",
        " rm ",
        " mv ",
        " cp ",
    ]
    .iter()
    .any(|marker| folded.contains(marker));
    let read_only = [
        "get-content",
        "get-childitem",
        "select-string",
        "test-path",
        "git status",
        "git diff",
        "git log",
        "rg ",
        "cargo check",
        "cargo test",
        "npm test",
        "npm run typecheck",
    ]
    .iter()
    .any(|marker| folded.contains(marker));

    match (read_only, mutating) {
        (true, true) => OperationKind::Mixed,
        (true, false) => OperationKind::ReadOnly,
        (false, true) => OperationKind::Mutating,
        (false, false) => OperationKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENSITIVE: &str = "PRIVATE_MARKER_command_C:\\secret\\source.rs";

    #[test]
    fn expands_exec_wrapper_without_returning_arguments() {
        let raw = format!(
            "const a = await tools.shell_command({{command: '{SENSITIVE}'}}); await tools.apply_patch('x');"
        );
        let result = classify(ToolInvocation {
            tool_name: "exec",
            raw_arguments: Some(&raw),
        });

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].tool_name, "shell_command");
        assert_eq!(result[0].category, ToolCategory::Execute);
        assert_eq!(result[1].tool_name, "apply_patch");
        assert_eq!(result[1].category, ToolCategory::Edit);
        let serialized = serde_json::to_string(&result).expect("serialize classifications");
        assert!(!serialized.contains(SENSITIVE));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn distinguishes_read_only_and_mutating_shell_activity() {
        let read = classify_one("shell_command", Some("Get-Content README.md; git status"));
        let write = classify_one("shell_command", Some("Set-Content private.txt value"));
        let mixed = classify_one(
            "shell_command",
            Some("Get-Content old.txt; Move-Item old.txt new.txt"),
        );

        assert_eq!(read.operation_kind, OperationKind::ReadOnly);
        assert_eq!(write.operation_kind, OperationKind::Mutating);
        assert_eq!(mixed.operation_kind, OperationKind::Mixed);
    }

    #[test]
    fn classifies_direct_tools_and_sanitizes_dynamic_names() {
        let result = classify(ToolInvocation {
            tool_name: "apply_patch <private>",
            raw_arguments: None,
        });
        assert_eq!(result[0].tool_name, "apply_patch__private_");
        assert_eq!(result[0].category, ToolCategory::Edit);
        assert_eq!(result[0].operation_kind, OperationKind::Mutating);
    }
}
