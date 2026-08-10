use std::rc::Rc;

/// Kind of include directive
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncludeKind {
    /// Local include with quotes: #include "file.h"
    Local,
    /// System include with angles: #include <file.h>
    System,
}

/// Snapshot of the include state passed to a custom [`IncludeResolver`] callback.
///
/// Contains the current include stack (for cycle detection) and the list of directories being
/// searched for includes.
#[derive(Clone, Debug, Default)]
pub struct IncludeContext {
    /// Stack of currently included files for cycle detection and context
    pub include_stack: Vec<String>,

    /// List of include directories to search
    pub include_dirs: Vec<String>,
}

/// Callback for resolving `#include` directives.
///
/// Called when the preprocessor encounters an `#include` after filesystem search paths have been
/// exhausted.
///
/// Return `Some(content)` to provide the file contents, or `None` to fall through to the
/// filesystem resolver.
///
/// - `path`: the header name from the directive (e.g. `"foo.h"` or `foo.h`)
/// - `kind`: whether this is a local (`"..."`) or system (`<...>`) include
/// - `context`: current include stack and search directories
pub type IncludeResolver = Rc<dyn Fn(&str, IncludeKind, &IncludeContext) -> Option<String>>;

/// Callback invoked on `#warning` directives (GCC/Clang only).
///
/// The `&str` parameter contains the raw warning message text.
pub type WarningHandler = Rc<dyn Fn(&str)>;

/// Target operating system for preprocessing
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Linux operating system
    Linux,
    /// Windows operating system
    Windows,
    /// macOS operating system
    MacOS,
}

/// Line ending style for output
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineEnding {
    /// Line Feed (`\n`) - Unix, Linux, macOS
    #[default]
    LF,
    /// Carriage Return + Line Feed (`\r\n`) - Windows
    CRLF,
    /// Carriage Return (`\r`) - classic Mac (pre-OS X)
    CR,
}

/// Compiler dialect for preprocessing
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compiler {
    /// GNU Compiler Collection
    GCC,
    /// LLVM Clang compiler
    Clang,
    /// Microsoft Visual C++ compiler
    MSVC,
}

/// C language standard used for standard predefined macros.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CStandard {
    /// ISO C90/C89. `__STDC_VERSION__` is not defined.
    C90,
    /// ISO C95 amendment. `__STDC_VERSION__` is `199409L`.
    C95,
    /// ISO C99. `__STDC_VERSION__` is `199901L`.
    C99,
    /// ISO C11. `__STDC_VERSION__` is `201112L`.
    #[default]
    C11,
    /// ISO C17/C18. `__STDC_VERSION__` is `201710L`.
    C17,
    /// ISO C23. `__STDC_VERSION__` is `202311L`.
    C23,
}

impl CStandard {
    pub(crate) const fn version_macro(self) -> Option<&'static str> {
        match self {
            Self::C90 => None,
            Self::C95 => Some("199409L"),
            Self::C99 => Some("199901L"),
            Self::C11 => Some("201112L"),
            Self::C17 => Some("201710L"),
            Self::C23 => Some("202311L"),
        }
    }
}

/// Execution environment used for the `__STDC_HOSTED__` macro.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecutionEnvironment {
    /// A hosted implementation with the standard library available.
    #[default]
    Hosted,

    /// A freestanding implementation without the hosted standard library.
    Freestanding,
}

/// Configuration for the C preprocessor
pub struct PreprocessorConfig {
    /// Target operating system
    pub target: Target,

    /// Compiler dialect
    pub compiler: Compiler,

    /// C language standard used for standard predefined macros.
    pub standard: CStandard,

    /// Hosted or freestanding execution environment.
    pub environment: ExecutionEnvironment,

    /// Maximum recursion depth for macro expansion
    pub recursion_limit: usize,

    /// Custom include file resolver function
    ///
    /// This is consulted *before* the built-in filesystem search path. Returning `None` falls back
    /// to searching `include_dirs` (and, for local includes, the directory of the including file).
    pub include_resolver: Option<IncludeResolver>,

    /// Ordered list of directories to search for `#include`/`#include_next`.
    ///
    /// For local includes (`#include "x"`), the directory of the including file is searched first,
    /// then these directories in order. For system includes (`#include <x>`), only these
    /// directories are searched.
    pub include_dirs: Vec<String>,

    /// Optional warning handler for #warning directives
    pub warning_handler: Option<WarningHandler>,

    /// Line ending style for output
    pub line_ending: LineEnding,
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
            standard: CStandard::C11,
            environment: ExecutionEnvironment::Hosted,
            recursion_limit: 128,
            include_resolver: None,
            include_dirs: Vec::new(),
            warning_handler: None,
            line_ending: LineEnding::LF,
        }
    }

    /// Create configuration for Windows + MSVC
    #[must_use]
    pub const fn for_windows() -> Self {
        Self {
            target: Target::Windows,
            compiler: Compiler::MSVC,
            standard: CStandard::C11,
            environment: ExecutionEnvironment::Hosted,
            recursion_limit: 128,
            include_resolver: None,
            include_dirs: Vec::new(),
            warning_handler: None,
            line_ending: LineEnding::CRLF,
        }
    }

    /// Create configuration for macOS + Clang
    #[must_use]
    pub const fn for_macos() -> Self {
        Self {
            target: Target::MacOS,
            compiler: Compiler::Clang,
            standard: CStandard::C11,
            environment: ExecutionEnvironment::Hosted,
            recursion_limit: 128,
            include_resolver: None,
            include_dirs: Vec::new(),
            warning_handler: None,
            line_ending: LineEnding::LF,
        }
    }

    /// Override the compiler for this configuration
    #[must_use]
    pub const fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.compiler = compiler;
        self
    }

    /// Set the C language standard used by the preprocessor.
    #[must_use]
    pub const fn with_standard(mut self, standard: CStandard) -> Self {
        self.standard = standard;
        self
    }

    /// Set whether the target is hosted or freestanding.
    #[must_use]
    pub const fn with_environment(mut self, environment: ExecutionEnvironment) -> Self {
        self.environment = environment;
        self
    }

    /// Set a warning handler for #warning directives
    #[must_use]
    pub fn with_warning_handler(mut self, handler: WarningHandler) -> Self {
        self.warning_handler = Some(handler);
        self
    }

    /// Set the line ending style for output
    #[must_use]
    pub const fn with_line_ending(mut self, ending: LineEnding) -> Self {
        self.line_ending = ending;
        self
    }

    /// Add a directory to the include search path.
    ///
    /// Directories are searched in the order they are added. For `#include "x"` the including
    /// file's directory is searched before these.
    #[must_use]
    pub fn with_include_dir(mut self, dir: impl Into<String>) -> Self {
        self.include_dirs.push(dir.into());
        self
    }

    /// Set the full list of include search directories.
    #[must_use]
    pub fn with_include_dirs(mut self, dirs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include_dirs = dirs.into_iter().map(Into::into).collect();
        self
    }
}
