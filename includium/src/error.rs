use std::fmt;

/// Semantic error kinds that can occur during preprocessing
#[derive(Debug)]
pub enum PreprocessErrorKind {
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

/// Errors that can occur during preprocessing, with location information
#[derive(Debug)]
pub struct PreprocessError {
    /// The specific kind of error that occurred
    pub kind: PreprocessErrorKind,
    /// Source file where the error occurred
    pub file: String,
    /// Line number where the error occurred
    pub line: usize,
    /// Optional column number for more precise location
    pub column: Option<usize>,
}

impl PreprocessError {
    /// Create an include not found error
    #[inline]
    pub fn include_not_found(file: String, line: usize, path: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::IncludeNotFound(path),
            file,
            line,
            column: None,
        }
    }

    /// Create a malformed directive error
    #[inline]
    pub fn malformed_directive(file: String, line: usize, directive: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::MalformedDirective(directive),
            file,
            line,
            column: None,
        }
    }

    /// Create a macro argument mismatch error
    #[inline]
    pub fn macro_arg_mismatch(file: String, line: usize, details: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::MacroArgMismatch(details),
            file,
            line,
            column: None,
        }
    }

    /// Create a recursion limit exceeded error
    #[inline]
    pub fn recursion_limit_exceeded(file: String, line: usize, details: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::RecursionLimitExceeded(details),
            file,
            line,
            column: None,
        }
    }

    /// Create a conditional compilation error
    #[inline]
    pub fn conditional_error(file: String, line: usize, details: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::ConditionalError(details),
            file,
            line,
            column: None,
        }
    }

    /// Create an I/O error
    #[inline]
    pub fn io_error(file: String, line: usize, error: std::io::Error) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::Io(error),
            file,
            line,
            column: None,
        }
    }

    /// Create a generic other error
    #[inline]
    pub fn other(file: String, line: usize, message: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::Other(message),
            file,
            line,
            column: None,
        }
    }

    /// Set column information for more precise error location
    #[must_use]
    pub fn with_column(mut self, column: usize) -> Self {
        self.column = Some(column);
        self
    }
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let loc = if let Some(col) = self.column {
            format!("{}:{}:{}", self.file, self.line, col)
        } else {
            format!("{}:{}", self.file, self.line)
        };

        match &self.kind {
            PreprocessErrorKind::IncludeNotFound(path) => {
                write!(f, "{}: include not found: {}", loc, path)
            }
            PreprocessErrorKind::MalformedDirective(directive) => {
                write!(f, "{}: malformed directive: {}", loc, directive)
            }
            PreprocessErrorKind::MacroArgMismatch(details) => {
                write!(f, "{}: macro argument mismatch: {}", loc, details)
            }
            PreprocessErrorKind::RecursionLimitExceeded(details) => {
                write!(f, "{}: recursion limit exceeded: {}", loc, details)
            }
            PreprocessErrorKind::ConditionalError(details) => {
                write!(f, "{}: conditional error: {}", loc, details)
            }
            PreprocessErrorKind::Io(err) => {
                write!(f, "{}: I/O error: {}", loc, err)
            }
            PreprocessErrorKind::Other(message) => {
                write!(f, "{}: error: {}", loc, message)
            }
        }
    }
}

impl std::error::Error for PreprocessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            PreprocessErrorKind::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PreprocessError {
    fn from(err: std::io::Error) -> Self {
        // For I/O errors without specific location context, use generic location
        PreprocessError::io_error("<internal>".to_string(), 0, err)
    }
}
