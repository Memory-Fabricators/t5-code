pub const BASH_TOOL_NAME: &str = "Bash";
pub const FILE_READ_TOOL_NAME: &str = "Read";
pub const FILE_WRITE_TOOL_NAME: &str = "Write";
pub const FILE_EDIT_TOOL_NAME: &str = "Edit";
pub const GLOB_TOOL_NAME: &str = "Glob";
pub const GREP_TOOL_NAME: &str = "Grep";
pub const TODO_WRITE_TOOL_NAME: &str = "TodoWrite";
pub const TASK_CREATE_TOOL_NAME: &str = "TaskCreate";
pub const ASK_USER_QUESTION_TOOL_NAME: &str = "AskUserQuestion";
pub const ENTER_PLAN_MODE_TOOL_NAME: &str = "EnterPlanMode";
pub const EXIT_PLAN_MODE_TOOL_NAME: &str = "ExitPlanMode";
pub const WEB_FETCH_TOOL_NAME: &str = "WebFetch";
pub const WEB_SEARCH_TOOL_NAME: &str = "WebSearch";

/// Default tool names for -p / early boot.
pub fn default_enabled_tool_names() -> Vec<String> {
    [
        BASH_TOOL_NAME,
        FILE_READ_TOOL_NAME,
        FILE_WRITE_TOOL_NAME,
        FILE_EDIT_TOOL_NAME,
        GLOB_TOOL_NAME,
        GREP_TOOL_NAME,
        TODO_WRITE_TOOL_NAME,
        ASK_USER_QUESTION_TOOL_NAME,
        ENTER_PLAN_MODE_TOOL_NAME,
        EXIT_PLAN_MODE_TOOL_NAME,
        WEB_FETCH_TOOL_NAME,
        WEB_SEARCH_TOOL_NAME,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
