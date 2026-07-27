use std::error;
use std::fmt;
use std::io;

/// Semantic error kinds that can occur during preprocessing
#[derive(Debug)]
pub enum PreprocessErrorKind {
    /// Include file not found. The `String` contains the requested header path.
    IncludeNotFound(String),
    /// Malformed preprocessor directive. The `String` contains the directive name (e.g. `"define"`).
    MalformedDirective(String),
    /// Macro argument count mismatch. The `String` contains a description of the mismatch.
    MacroArgMismatch(String),
    /// Macro expansion recursion limit exceeded. The `String` contains diagnostic details.
    RecursionLimitExceeded(String),
    /// Conditional compilation error (e.g. `#elif without #if`). The `String` contains the details.
    ConditionalError(String),
    /// I/O error (e.g. file reading/writing during `#include` resolution).
    Io(io::Error),
    /// Catch-all for other preprocessing errors. The `String` contains the error message.
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
    /// Optional source line content for context display
    pub source_line: Option<String>,
}

impl PreprocessError {
    /// Create an include-not-found error.
    ///
    /// - `file`: source file containing the `#include` directive
    /// - `line`: 1-based line number of the directive
    /// - `path`: header name that was not found (e.g. `"foo.h"`)
    #[inline]
    pub fn include_not_found(file: String, line: usize, path: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::IncludeNotFound(path),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Create a malformed-directive error.
    ///
    /// - `file`: source file containing the malformed directive
    /// - `line`: 1-based line number
    /// - `directive`: directive name that was malformed (e.g. `"define"`)
    #[inline]
    pub fn malformed_directive(file: String, line: usize, directive: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::MalformedDirective(directive),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Create a macro-argument-mismatch error.
    ///
    /// - `file`: source file where the macro was invoked
    /// - `line`: 1-based line number
    /// - `details`: description of the expected vs actual argument count
    #[inline]
    pub fn macro_arg_mismatch(file: String, line: usize, details: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::MacroArgMismatch(details),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Create a recursion-limit-exceeded error.
    ///
    /// - `file`: source file where the recursion was triggered
    /// - `line`: 1-based line number
    /// - `details`: diagnostic details (e.g. macro name)
    #[inline]
    pub fn recursion_limit_exceeded(file: String, line: usize, details: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::RecursionLimitExceeded(details),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Create a conditional-compilation error.
    ///
    /// - `file`: source file containing the malformed conditional
    /// - `line`: 1-based line number
    /// - `details`: description (e.g. `"#elif without #if"`)
    #[inline]
    pub fn conditional_error(file: String, line: usize, details: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::ConditionalError(details),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Create an I/O error.
    ///
    /// - `file`: source file being processed when the I/O error occurred
    /// - `line`: 1-based line number
    /// - `error`: the underlying [`io::Error`]
    #[inline]
    pub fn io_error(file: String, line: usize, error: io::Error) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::Io(error),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Create a generic error.
    ///
    /// - `file`: source file (or a synthetic identifier like `"<internal>"`)
    /// - `line`: 1-based line number (or `0` for synthetic locations)
    /// - `message`: error message
    #[inline]
    pub fn other(file: String, line: usize, message: String) -> Self {
        PreprocessError {
            kind: PreprocessErrorKind::Other(message),
            file,
            line,
            column: None,
            source_line: None,
        }
    }

    /// Set the 1-based column number for a more precise error location.
    #[must_use]
    pub const fn with_column(mut self, column: usize) -> Self {
        self.column = Some(column);
        self
    }

    /// Set the source line for context display.
    ///
    /// When set, the error message includes the source line with a caret (`^`)
    /// indicator pointing at the column position (if also set via [`Self::with_column`]).
    #[must_use]
    pub fn with_source_line(mut self, source_line: String) -> Self {
        self.source_line = Some(source_line);
        self
    }
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let is_fake_location = self.file.starts_with('<') || self.line == 0;

        let message = match &self.kind {
            PreprocessErrorKind::IncludeNotFound(path) => {
                format!("include not found: {path}")
            }
            PreprocessErrorKind::MalformedDirective(directive) => {
                format!("malformed directive: {directive}")
            }
            PreprocessErrorKind::MacroArgMismatch(details) => {
                format!("macro argument mismatch: {details}")
            }
            PreprocessErrorKind::RecursionLimitExceeded(details) => {
                format!("recursion limit exceeded: {details}")
            }
            PreprocessErrorKind::ConditionalError(details) => {
                format!("conditional error: {details}")
            }
            PreprocessErrorKind::Io(err) => {
                format!("I/O error: {err}")
            }
            PreprocessErrorKind::Other(msg) => msg.clone(),
        };

        if is_fake_location {
            // For internal/synthetic locations, show brief error with context for maintainers
            write!(
                f,
                "preprocessor error ({}:{}): {message}",
                self.file, self.line
            )?;
        } else {
            let loc = if let Some(col) = self.column {
                format!("{}:{}:{col}", self.file, self.line)
            } else {
                format!("{}:{}", self.file, self.line)
            };
            write!(f, "{loc}: {message}")?;
        }

        if let (Some(col), Some(source_line)) = (self.column, &self.source_line) {
            write!(f, "\n{source_line}\n")?;
            let indent = " ".repeat(col.saturating_sub(1));
            write!(f, "{indent}^")?;
        }

        Ok(())
    }
}

impl error::Error for PreprocessError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match &self.kind {
            PreprocessErrorKind::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for PreprocessError {
    fn from(err: io::Error) -> Self {
        // For I/O errors without specific location context, use generic location
        PreprocessError::io_error("<internal>".to_string(), 0, err)
    }
}
