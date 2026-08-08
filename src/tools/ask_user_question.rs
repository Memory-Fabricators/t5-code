//! Port of AskUserQuestionTool.

use serde_json::{Value, json};

use crate::constants::tool_names::ASK_USER_QUESTION_TOOL_NAME;
use crate::tools::ToolResult;
use crate::tools::ToolSession;
use crate::tools::interaction::{Question, QuestionOption, UserPrompt, UserResponse};

pub fn api_schema() -> Value {
    json!({
        "name": ASK_USER_QUESTION_TOOL_NAME,
        "description": "Ask the user clarifying questions with multiple-choice options. Use when you need decisions before proceeding.",
        "input_schema": {
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": { "type": "string" },
                            "header": { "type": "string" },
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    },
                                    "required": ["label", "description"]
                                }
                            },
                            "multiSelect": { "type": "boolean" }
                        },
                        "required": ["question", "header", "options"]
                    }
                }
            },
            "required": ["questions"]
        }
    })
}

pub async fn execute(input: &Value, session: &mut ToolSession) -> ToolResult {
    let Some(arr) = input.get("questions").and_then(|v| v.as_array()) else {
        return ToolResult {
            content: "AskUserQuestion requires a questions array".into(),
            is_error: true,
        };
    };

    let mut questions = Vec::new();
    for q in arr {
        let question = q
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let header = q
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("Question")
            .to_string();
        let multi_select = q
            .get("multiSelect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let options = q
            .get("options")
            .and_then(|v| v.as_array())
            .map(|opts| {
                opts.iter()
                    .map(|o| QuestionOption {
                        label: o
                            .get("label")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: o
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if question.is_empty() || options.len() < 2 {
            continue;
        }
        questions.push(Question {
            question,
            header,
            options,
            multi_select,
        });
    }

    if questions.is_empty() {
        return ToolResult {
            content: "No valid questions provided".into(),
            is_error: true,
        };
    }

    // If answers already present (pre-filled), pass through.
    if let Some(answers) = input.get("answers").and_then(|v| v.as_object()) {
        let formatted: Vec<String> = answers
            .iter()
            .map(|(k, v)| format!("\"{k}\"={v}"))
            .collect();
        return ToolResult {
            content: format!("User has answered your questions: {}", formatted.join(", ")),
            is_error: false,
        };
    }

    let Some(bridge) = session.interaction.as_ref() else {
        return ToolResult {
            content: "AskUserQuestion requires an interactive session".into(),
            is_error: true,
        };
    };

    let response = bridge.ask(UserPrompt::AskUser { questions }).await;

    match response {
        UserResponse::AskUser { answers } => {
            let formatted: Vec<String> = answers
                .iter()
                .map(|(k, v)| format!("\"{k}\"=\"{v}\""))
                .collect();
            ToolResult {
                content: format!("User has answered your questions: {}", formatted.join(", ")),
                is_error: false,
            }
        }
        UserResponse::Cancelled | UserResponse::ExitPlan { .. } => ToolResult {
            content: "User declined to answer questions.".into(),
            is_error: true,
        },
    }
}
