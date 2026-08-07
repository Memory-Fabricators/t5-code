//! Port of `src/utils/errors.ts` (partial).

use std::fmt;

/// Thrown when the global config JSON cannot be parsed.
#[derive(Debug)]
pub struct ConfigParseError {
    pub message: String,
    pub file_path: String,
}

impl ConfigParseError {
    pub fn new(message: impl Into<String>, file_path: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file_path: file_path.into(),
        }
    }
}

impl fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ConfigParseError at {}: {}",
            self.file_path, self.message
        )
    }
}

impl std::error::Error for ConfigParseError {}
