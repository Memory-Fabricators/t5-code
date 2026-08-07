//! Port of TodoWriteTool — in-memory session todo list.

use serde_json::{Value, json};

use super::{TodoItem, ToolResult};
use crate::constants::tool_names::{FILE_EDIT_TOOL_NAME, TODO_WRITE_TOOL_NAME};

const DESCRIPTION: &str = "Update the todo list for the current session. To be used proactively and often to track progress and pending tasks. Make sure that at least one task is in_progress at all times. Always provide both content (imperative) and activeForm (present continuous) for each task.";

/// Full tool prompt from `TodoWriteTool/prompt.ts` (trimmed examples kept for behavior).
fn prompt() -> String {
    format!(
        "Use this tool to create and manage a structured task list for your current coding session. This helps you track progress, organize complex tasks, and demonstrate thoroughness to the user.\n\
It also helps the user understand the progress of the task and overall progress of their requests.\n\
\n\
## When to Use This Tool\n\
Use this tool proactively in these scenarios:\n\
\n\
1. Complex multi-step tasks - When a task requires 3 or more distinct steps or actions\n\
2. Non-trivial and complex tasks - Tasks that require careful planning or multiple operations\n\
3. User explicitly requests todo list - When the user directly asks you to use the todo list\n\
4. User provides multiple tasks - When users provide a list of things to be done (numbered or comma-separated)\n\
5. After receiving new instructions - Immediately capture user requirements as todos\n\
6. When you start working on a task - Mark it as in_progress BEFORE beginning work. Ideally you should only have one todo as in_progress at a time\n\
7. After completing a task - Mark it as completed and add any new follow-up tasks discovered during implementation\n\
\n\
## When NOT to Use This Tool\n\
\n\
Skip using this tool when:\n\
1. There is only a single, straightforward task\n\
2. The task is trivial and tracking it provides no organizational benefit\n\
3. The task can be completed in less than 3 trivial steps\n\
4. The task is purely conversational or informational\n\
\n\
NOTE that you should not use this tool if there is only one trivial task to do. In this case you are better off just doing the task directly.\n\
\n\
## Task States and Management\n\
\n\
1. **Task States**: Use these states to track progress:\n\
   - pending: Task not yet started\n\
   - in_progress: Currently working on (limit to ONE task at a time)\n\
   - completed: Task finished successfully\n\
\n\
   **IMPORTANT**: Task descriptions must have two forms:\n\
   - content: The imperative form describing what needs to be done (e.g., \"Run tests\", \"Build the project\")\n\
   - activeForm: The present continuous form shown during execution (e.g., \"Running tests\", \"Building the project\")\n\
\n\
2. **Task Management**:\n\
   - Update task status in real-time as you work\n\
   - Mark tasks complete IMMEDIATELY after finishing (don't batch completions)\n\
   - Exactly ONE task must be in_progress at any time (not less, not more)\n\
   - Complete current tasks before starting new ones\n\
   - Remove tasks that are no longer relevant from the list entirely\n\
\n\
3. **Task Completion Requirements**:\n\
   - ONLY mark a task as completed when you have FULLY accomplished it\n\
   - If you encounter errors, blockers, or cannot finish, keep the task as in_progress\n\
   - When blocked, create a new task describing what needs to be resolved\n\
   - Never mark a task as completed if tests are failing, implementation is partial, or errors remain\n\
\n\
4. **Task Breakdown**:\n\
   - Create specific, actionable items\n\
   - Break complex tasks into smaller, manageable steps\n\
   - Always provide both forms: content (imperative) and activeForm (present continuous)\n\
\n\
When in doubt, use this tool. Prefer dedicated file tools ({FILE_EDIT_TOOL_NAME}, etc.) for the actual work."
    )
}

pub fn api_schema() -> Value {
    json!({
        "name": TODO_WRITE_TOOL_NAME,
        "description": format!("{DESCRIPTION}\n\n{}", prompt()),
        "input_schema": {
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "minLength": 1,
                                "description": "Imperative form of the task (e.g. \"Run tests\")"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            },
                            "activeForm": {
                                "type": "string",
                                "minLength": 1,
                                "description": "Present continuous form (e.g. \"Running tests\")"
                            }
                        },
                        "required": ["content", "status", "activeForm"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["todos"],
            "additionalProperties": false
        }
    })
}

pub fn execute(input: &Value, todos: &mut Vec<TodoItem>) -> ToolResult {
    let Some(arr) = input.get("todos").and_then(|v| v.as_array()) else {
        return ToolResult {
            content: "Missing required parameter: todos".into(),
            is_error: true,
        };
    };

    let mut parsed: Vec<TodoItem> = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let content = match item.get("content").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return ToolResult {
                    content: format!("todos[{i}].content must be a non-empty string"),
                    is_error: true,
                };
            }
        };
        let active_form = match item.get("activeForm").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return ToolResult {
                    content: format!("todos[{i}].activeForm must be a non-empty string"),
                    is_error: true,
                };
            }
        };
        let status = match item.get("status").and_then(|v| v.as_str()) {
            Some("pending" | "in_progress" | "completed") => {
                item.get("status").unwrap().as_str().unwrap().to_string()
            }
            _ => {
                return ToolResult {
                    content: format!(
                        "todos[{i}].status must be pending, in_progress, or completed"
                    ),
                    is_error: true,
                };
            }
        };
        parsed.push(TodoItem {
            content,
            status,
            active_form,
        });
    }

    let all_done = !parsed.is_empty() && parsed.iter().all(|t| t.status == "completed");
    *todos = if all_done { Vec::new() } else { parsed };

    ToolResult {
        content: "Todos have been modified successfully. Ensure that you continue to use the todo list to track your progress. Please proceed with the current tasks if applicable".into(),
        is_error: false,
    }
}
