use std::fmt;

/// Errors that can occur during preprocessing
#[derive(Debug)]
pub enum PreprocessError {
    /// Include file not found
    IncludeNotFound(String),
    /// Malformed preprocessor directive
    MalformedDirective(String),
    /// Macro argument count mismatch
    MacroArgMismatch(String),
    /// Macro expansion recursion limit exceeded
    RecursionLimitExceeded(String),
    /// Conditional compilation error
    ConditionalError(String),
    /// I/O error (e.g., file reading/writing)
    Io(std::io::Error),
    /// Other preprocessing error
    Other(String),
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreprocessError::IncludeNotFound(s) => write!(f, "include not found: {s}"),
            PreprocessError::MalformedDirective(s) => write!(f, "malformed directive: {s}"),
            PreprocessError::MacroArgMismatch(s) => write!(f, "macro arg mismatch: {s}"),
            PreprocessError::RecursionLimitExceeded(s) => write!(f, "recursion limit: {s}"),
            PreprocessError::ConditionalError(s) => write!(f, "conditional error: {s}"),
            PreprocessError::Io(err) => write!(f, "I/O error: {err}"),
            PreprocessError::Other(s) => write!(f, "error: {s}"),
        }
    }
}
impl std::error::Error for PreprocessError {}

impl From<std::io::Error> for PreprocessError {
    fn from(err: std::io::Error) -> Self {
        PreprocessError::Io(err)
    }
}
