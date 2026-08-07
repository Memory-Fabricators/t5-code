//! Port of `src/constants/prompts.ts` (stub).

use crate::PortResult;

pub async fn get_system_prompt(_tools: &[String], _model: &str) -> PortResult<Vec<String>> {
    Err(crate::not_yet_ported("constants/prompts"))
}
