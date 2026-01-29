use std::collections::HashMap;
use std::collections::HashSet;

use crate::config::{Compiler, IncludeResolver, Target, WarningHandler};
use crate::macro_def::Macro;

/// State for conditional compilation directives
#[derive(Clone, Debug)]
pub struct ConditionalState {
    /// Whether the current branch is active and its code should be emitted
    pub is_active: bool,
    /// Whether any branch in this #if/#endif block has been taken already
    pub any_branch_taken: bool,
}

impl ConditionalState {
    /// Create a new conditional state for an #if/#ifdef/#ifndef
    pub fn new(active: bool) -> Self {
        Self {
            is_active: active,
            any_branch_taken: active,
        }
    }
}

/// Context containing all state for preprocessor operations
///
/// This struct holds all mutable state needed during preprocessing,
/// making it easy to test and reuse the preprocessor logic.
pub struct PreprocessorContext {
    /// Defined macros
    pub macros: HashMap<String, Macro>,

    /// Macros temporarily disabled during expansion (to prevent recursion)
    pub disabled_macros: HashSet<String>,

    /// Files included with #pragma once
    pub included_once: HashSet<String>,

    /// Stack of currently included files for cycle detection
    pub include_stack: Vec<String>,

    /// Custom include resolver function
    pub include_resolver: Option<IncludeResolver>,

    /// Stack of conditional compilation states
    pub conditional_stack: Vec<ConditionalState>,

    /// Current file name for error reporting and __FILE__ macro
    pub current_file: String,

    /// Current line number for __LINE__ macro
    pub current_line: usize,

    /// Maximum recursion depth for macro expansion
    pub recursion_limit: usize,

    /// Compiler dialect for preprocessing
    pub compiler: Compiler,

    /// Optional warning handler for #warning directives
    pub warning_handler: Option<WarningHandler>,
}

impl Default for PreprocessorContext {
    fn default() -> Self {
        Self::new()
    }
}

impl PreprocessorContext {
    /// Create a new preprocessor context with defaults
    #[must_use]
    pub fn new() -> Self {
        PreprocessorContext {
            macros: HashMap::new(),
            disabled_macros: HashSet::new(),
            included_once: HashSet::new(),
            include_stack: Vec::new(),
            include_resolver: None,
            conditional_stack: Vec::new(),
            current_file: "<stdin>".to_string(),
            current_line: 1,
            recursion_limit: 128,
            compiler: Compiler::GCC,
            warning_handler: None,
        }
    }

    /// Apply configuration to the context
    pub fn apply_config(&mut self, config: &crate::config::PreprocessorConfig) {
        self.compiler = config.compiler.clone();
        self.recursion_limit = config.recursion_limit;
        self.include_resolver.clone_from(&config.include_resolver);
        self.warning_handler.clone_from(&config.warning_handler);

        self.define_target_macros(&config.target);
        self.define_compiler_macros(&config.compiler);

        self.stub_compiler_intrinsics();
        self.define_sizeof_stubs();
    }

    fn define_target_macros(&mut self, target: &Target) {
        match target {
            Target::Linux => {
                self.define_builtin("__linux__", None, "1", false);
                self.define_builtin("__unix__", None, "1", false);
                self.define_builtin("__LP64__", None, "1", false);
            }
            Target::Windows => {
                self.define_builtin("_WIN32", None, "1", false);
                self.define_builtin("WIN32", None, "1", false);
                self.define_builtin("_WINDOWS", None, "1", false);
            }
            Target::MacOS => {
                self.define_builtin("__APPLE__", None, "1", false);
                self.define_builtin("__MACH__", None, "1", false);
                self.define_builtin("TARGET_OS_MAC", None, "1", false);
                self.define_builtin("__LP64__", None, "1", false);
            }
        }
    }

    fn define_compiler_macros(&mut self, compiler: &Compiler) {
        match compiler {
            Compiler::GCC => {
                // GCC 11.2.0
                self.define_builtin("__GNUC__", None, "11", false);
                self.define_builtin("__GNUC_MINOR__", None, "2", false);
                self.define_builtin("__GNUC_PATCHLEVEL__", None, "0", false);
                self.define_builtin("_GNU_SOURCE", None, "1", false);
            }
            Compiler::Clang => {
                // Clang 14.0.0
                self.define_builtin("__clang__", None, "1", false);
                self.define_builtin("__clang_major__", None, "14", false);
                self.define_builtin("__clang_minor__", None, "0", false);
                self.define_builtin("__clang_patchlevel__", None, "0", false);
            }
            Compiler::MSVC => {
                // MSVC 19.20 (Visual Studio 2019)
                self.define_builtin("_MSC_VER", None, "1920", false);
                self.define_builtin("_MSC_FULL_VER", None, "192027508", false);
                self.define_builtin("WIN32_LEAN_AND_MEAN", None, "", false);
                self.define_builtin("_CRT_SECURE_NO_WARNINGS", None, "", false);
            }
        }
    }

    fn stub_compiler_intrinsics(&mut self) {
        // Stub __builtin_* macros to prevent errors
        self.define_builtin("__builtin_expect", None, "", false);
        self.define_builtin("__builtin_unreachable", None, "", false);
        self.define_builtin("__builtin_va_start", None, "", false);
        self.define_builtin("__builtin_va_arg", None, "", false);
        self.define_builtin("__builtin_va_end", None, "", false);
    }

    fn define_sizeof_stubs(&mut self) {
        // Define common sizeof values as stubs
        self.define_builtin("__SIZEOF_INT__", None, "4", false);
        self.define_builtin("__SIZEOF_LONG__", None, "8", false);
        self.define_builtin("__SIZEOF_LONG_LONG__", None, "8", false);
        self.define_builtin("__SIZEOF_POINTER__", None, "8", false);
        self.define_builtin("__SIZEOF_SIZE_T__", None, "8", false);
        self.define_builtin("__SIZEOF_PTRDIFF_T__", None, "8", false);
    }

    /// Define a preprocessor macro
    pub fn define<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
    ) {
        self.define_macro(name, params, body, is_variadic, false);
    }

    fn define_builtin<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
    ) {
        self.define_macro(name, params, body, is_variadic, true);
    }

    fn define_macro<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
        is_builtin: bool,
    ) {
        use crate::engine::PreprocessorEngine;
        use std::rc::Rc;

        let stripped_body = PreprocessorEngine::strip_comments(body.as_ref());
        let body_tokens = PreprocessorEngine::tokenize_line(&stripped_body);
        self.macros.insert(
            name.as_ref().to_string(),
            Macro {
                params,
                body: Rc::new(body_tokens),
                is_variadic,
                definition_location: if is_builtin {
                    None
                } else {
                    Some((self.current_file.clone(), self.current_line))
                },
                is_builtin,
            },
        );
    }

    /// Remove a macro definition
    pub fn undef(&mut self, name: &str) {
        self.macros.remove(name);
    }

    /// Check if a macro is defined
    #[must_use]
    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    /// Get a reference to the defined macros
    #[must_use]
    pub fn get_macros(&self) -> &HashMap<String, Macro> {
        &self.macros
    }
}
