use std::rc::Rc;

/// Kind of include directive
#[derive(Clone, Debug, PartialEq)]
pub enum IncludeKind {
    /// Local include with quotes: #include "file.h"
    Local,
    /// System include with angles: #include <file.h>
    System,
}

/// Context for include resolution
#[derive(Clone, Debug, Default)]
pub struct IncludeContext {
    /// Stack of currently included files for cycle detection and context
    pub include_stack: Vec<String>,
    /// List of include directories to search
    pub include_dirs: Vec<String>,
}

/// Type alias for include resolver function
pub type IncludeResolver = Rc<dyn Fn(&str, IncludeKind, &IncludeContext) -> Option<String>>;

/// Type alias for warning handler function
pub type WarningHandler = Rc<dyn Fn(&str)>;

/// Target operating system for preprocessing
#[derive(Clone, Debug)]
pub enum Target {
    /// Linux operating system
    Linux,
    /// Windows operating system
    Windows,
    /// macOS operating system
    MacOS,
}

/// Compiler dialect for preprocessing
#[derive(Clone, Debug)]
pub enum Compiler {
    /// GNU Compiler Collection
    GCC,
    /// LLVM Clang compiler
    Clang,
    /// Microsoft Visual C++ compiler
    MSVC,
}

/// Configuration for the C preprocessor
pub struct PreprocessorConfig {
    /// Target operating system
    pub target: Target,
    /// Compiler dialect
    pub compiler: Compiler,
    /// Maximum recursion depth for macro expansion
    pub recursion_limit: usize,
    /// Custom include file resolver function
    pub include_resolver: Option<IncludeResolver>,
    /// Optional warning handler for #warning directives
    pub warning_handler: Option<WarningHandler>,
}

impl Default for PreprocessorConfig {
    fn default() -> Self {
        Self::for_linux()
    }
}

impl PreprocessorConfig {
    /// Create configuration for Linux + GCC
    #[must_use]
    pub const fn for_linux() -> Self {
        Self {
            target: Target::Linux,
            compiler: Compiler::GCC,
            recursion_limit: 128,
            include_resolver: None,
            warning_handler: None,
        }
    }

    /// Create configuration for Windows + MSVC
    #[must_use]
    pub const fn for_windows() -> Self {
        Self {
            target: Target::Windows,
            compiler: Compiler::MSVC,
            recursion_limit: 128,
            include_resolver: None,
            warning_handler: None,
        }
    }

    /// Create configuration for macOS + Clang
    #[must_use]
    pub const fn for_macos() -> Self {
        Self {
            target: Target::MacOS,
            compiler: Compiler::Clang,
            recursion_limit: 128,
            include_resolver: None,
            warning_handler: None,
        }
    }

    /// Override the compiler for this configuration
    #[must_use]
    pub const fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.compiler = compiler;
        self
    }

    /// Set a warning handler for #warning directives
    #[must_use]
    pub fn with_warning_handler(mut self, handler: WarningHandler) -> Self {
        self.warning_handler = Some(handler);
        self
    }
}
