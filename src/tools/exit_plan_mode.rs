//! Port of ExitPlanModeTool (simplified approve/reject).

use serde_json::{Value, json};

use crate::constants::tool_names::EXIT_PLAN_MODE_TOOL_NAME;
use crate::tools::interaction::{UserPrompt, UserResponse};
use crate::tools::{PermissionMode, ToolResult, ToolSession};
use crate::utils::plans::{get_plan, get_plan_file_path};

pub fn api_schema() -> Value {
    json!({
        "name": EXIT_PLAN_MODE_TOOL_NAME,
        "description": "Exit plan mode and request user approval of the plan before implementing.",
        "input_schema": {
            "type": "object",
            "properties": {
                "allowedPrompts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "prompt": { "type": "string" }
                        }
                    }
                }
            }
        }
    })
}

pub async fn execute(session: &mut ToolSession) -> ToolResult {
    if session.permission_mode != PermissionMode::Plan {
        return ToolResult {
            content: "ExitPlanMode can only be used while in plan mode. Call EnterPlanMode first."
                .into(),
            is_error: true,
        };
    }

    let path = session
        .plan_file_path
        .clone()
        .unwrap_or_else(get_plan_file_path);
    let plan = get_plan().unwrap_or_else(|| "(empty plan file)".into());

    let Some(bridge) = session.interaction.as_ref() else {
        // Non-interactive: auto-approve.
        session.permission_mode = session
            .pre_plan_mode
            .take()
            .unwrap_or(PermissionMode::Default);
        return ToolResult {
            content: format!("Exited plan mode (non-interactive auto-approve).\n\nPlan:\n{plan}"),
            is_error: false,
        };
    };

    let response = bridge
        .ask(UserPrompt::ExitPlan {
            plan: plan.clone(),
            path: path.display().to_string(),
        })
        .await;

    match response {
        UserResponse::ExitPlan { approved: true } => {
            session.permission_mode = session
                .pre_plan_mode
                .take()
                .unwrap_or(PermissionMode::Default);
            ToolResult {
                content: format!(
                    "User approved the plan. You have exited plan mode. Implement the plan now.\n\nPlan:\n{plan}"
                ),
                is_error: false,
            }
        }
        _ => ToolResult {
            content: "User rejected the plan. Stay in plan mode and revise.".into(),
            is_error: true,
        },
    }
}
