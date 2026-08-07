//! Tool name constants used in the system prompt (from various tools/*/ files).

pub const BASH_TOOL_NAME: &str = "Bash";
pub const FILE_READ_TOOL_NAME: &str = "Read";
pub const FILE_WRITE_TOOL_NAME: &str = "Write";
pub const FILE_EDIT_TOOL_NAME: &str = "Edit";
pub const GLOB_TOOL_NAME: &str = "Glob";
pub const GREP_TOOL_NAME: &str = "Grep";
pub const TODO_WRITE_TOOL_NAME: &str = "TodoWrite";
pub const TASK_CREATE_TOOL_NAME: &str = "TaskCreate";
pub const ASK_USER_QUESTION_TOOL_NAME: &str = "AskUserQuestion";

/// Default tool names for -p / early boot before the full tool registry is ported.
/// AskUserQuestion omitted — needs interactive elicitation.
pub fn default_enabled_tool_names() -> Vec<String> {
    [
        BASH_TOOL_NAME,
        FILE_READ_TOOL_NAME,
        FILE_WRITE_TOOL_NAME,
        FILE_EDIT_TOOL_NAME,
        GLOB_TOOL_NAME,
        GREP_TOOL_NAME,
        TODO_WRITE_TOOL_NAME,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
