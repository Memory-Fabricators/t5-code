//! Port of EnterPlanModeTool.

use serde_json::{Value, json};

use crate::constants::tool_names::ENTER_PLAN_MODE_TOOL_NAME;
use crate::tools::{PermissionMode, ToolResult, ToolSession};
use crate::utils::plans::ensure_plan_file;

pub fn api_schema() -> Value {
    json!({
        "name": ENTER_PLAN_MODE_TOOL_NAME,
        "description": "Enter plan mode to design an approach before making code changes. Use for complex multi-step tasks.",
        "input_schema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    })
}

pub fn execute(session: &mut ToolSession) -> ToolResult {
    if session.permission_mode == PermissionMode::Plan {
        return ToolResult {
            content: "Already in plan mode.".into(),
            is_error: false,
        };
    }
    session.pre_plan_mode = Some(session.permission_mode);
    session.permission_mode = PermissionMode::Plan;
    let path = ensure_plan_file();
    session.plan_file_path = Some(path.clone());
    session.inject_plan_reminder = true;

    ToolResult {
        content: format!(
            "Entered plan mode. Explore and design an approach, write the plan to {}, then use ExitPlanMode when ready for approval. Prefer AskUserQuestion to clarify trade-offs. Do not make unrelated code changes until the plan is approved.",
            path.display()
        ),
        is_error: false,
    }
}
