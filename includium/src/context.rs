use std::collections::{HashMap, HashSet};

use crate::config::{Architecture, Compiler, IncludeResolver, LineEnding, Target, WarningHandler};
use crate::macro_def::Macro;

use crate::config::{CStandard, ExecutionEnvironment};
use crate::{PreprocessorConfig, engine};
use std::rc::Rc;

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
    pub const fn new(active: bool) -> Self {
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

    /// Ordered directories searched for `#include`/`#include_next`
    pub include_dirs: Vec<String>,

    /// Stack of conditional compilation states
    pub conditional_stack: Vec<ConditionalState>,

    /// Per-macro save stack for `#pragma push_macro`/`pop_macro`.
    ///
    /// Each entry is a saved `Macro` definition (or `None` for "was undefined").
    pub macro_stack: HashMap<String, Vec<Option<Macro>>>,

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

    /// Line ending style for output denormalization
    pub line_ending: LineEnding,
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
            include_dirs: Vec::new(),
            conditional_stack: Vec::new(),
            macro_stack: HashMap::new(),
            current_file: "<stdin>".to_string(),
            current_line: 1,
            recursion_limit: 128,
            compiler: Compiler::GCC,
            warning_handler: None,
            line_ending: LineEnding::LF,
        }
    }

    /// Apply configuration to the context.
    ///
    /// This copies the configuration fields and also defines the built-in macros
    /// for the target platform, compiler, standard intrinsics, and `sizeof` stubs.
    pub fn apply_config(&mut self, config: &PreprocessorConfig) {
        self.compiler = config.compiler;
        self.recursion_limit = config.recursion_limit;
        self.include_resolver.clone_from(&config.include_resolver);
        self.include_dirs = config.include_dirs.clone();
        self.warning_handler.clone_from(&config.warning_handler);
        self.line_ending = config.line_ending;

        self.define_target_macros(config.target, config.architecture, config.compiler);
        self.define_compiler_macros(config.compiler, config.standard, config.environment);

        self.stub_compiler_intrinsics();
        self.define_sizeof_stubs();
    }

    fn define_target_macros(
        &mut self,
        target: Target,
        architecture: Architecture,
        compiler: Compiler,
    ) {
        match target {
            Target::Linux => {
                self.define_builtin("__linux__", None, "1", false);
                self.define_builtin("__unix__", None, "1", false);
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
            }
        }

        // Architecture macros depend on the compiler dialect: GCC/Clang expose `__x86_64__` and
        // `__i386__`, while MSVC exposes `_M_X64`/`_M_IX86`, and the LP64 model macros must be
        // omitted on the LLP64 Windows ABI.
        //
        // Emitting the wrong dialect's macros makes system headers misidentify the target and fail
        // to preprocess/compile.
        let is_msvc = compiler == Compiler::MSVC;

        match (architecture, is_msvc) {
            (Architecture::X86, false) => self.define_builtin("__i386__", None, "1", false),
            (Architecture::X86, true) => {
                self.define_builtin("_M_IX86", None, "600", false);
            }
            (Architecture::X86_64, false) => {
                self.define_builtin("__x86_64__", None, "1", false);
                // `__LP64__` only holds for the SysV LP64 ABIs (Linux, macOS);
                // Windows remains LLP64 even under MinGW, so it is not defined there.
                if target != Target::Windows {
                    self.define_builtin("__LP64__", None, "1", false);
                }
            }
            (Architecture::X86_64, true) => {
                self.define_builtin("_M_X64", None, "100", false);
                self.define_builtin("_M_AMD64", None, "100", false);
                self.define_builtin("_WIN64", None, "1", false);
            }
            (Architecture::Arm, false) => self.define_builtin("__arm__", None, "1", false),
            (Architecture::Arm, true) => self.define_builtin("_M_ARM", None, "700", false),
            (Architecture::Aarch64, false) => {
                self.define_builtin("__aarch64__", None, "1", false);
                if target != Target::Windows {
                    self.define_builtin("__LP64__", None, "1", false);
                }
            }
            (Architecture::Aarch64, true) => {
                self.define_builtin("_M_ARM64", None, "100", false);
            }
            (Architecture::Riscv32, false) => {
                self.define_builtin("__riscv", None, "1", false);
                self.define_builtin("__riscv_xlen", None, "32", false);
            }
            (Architecture::Riscv64, false) => {
                self.define_builtin("__riscv", None, "1", false);
                self.define_builtin("__riscv_xlen", None, "64", false);
                if target != Target::Windows {
                    self.define_builtin("__LP64__", None, "1", false);
                }
            }
            (Architecture::Riscv32, true) | (Architecture::Riscv64, true) => {
                // MSVC does not define RISC-V target macros; nothing to emit.
            }
            (Architecture::Unknown, _) => {}
        }
    }

    fn define_compiler_macros(
        &mut self,
        compiler: Compiler,
        standard: CStandard,
        environment: ExecutionEnvironment,
    ) {
        // These standard predefined macros are required by system headers before compiler-specific
        // feature checks are evaluated.
        self.define_builtin("__STDC__", None, "1", false);
        let hosted = match environment {
            ExecutionEnvironment::Hosted => "1",
            ExecutionEnvironment::Freestanding => "0",
        };
        self.define_builtin("__STDC_HOSTED__", None, hosted, false);
        if let Some(version) = standard.version_macro() {
            self.define_builtin("__STDC_VERSION__", None, version, false);
        }

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

    /// Define a preprocessor macro.
    ///
    /// - `name`: macro identifier (e.g. `"MAX"`)
    /// - `params`: `None` for an object-like macro, `Some(vec![...])` for a function-like macro
    /// - `body`: replacement text (e.g. `"((a) > (b) ? (a) : (b))"`)
    /// - `is_variadic`: if `true`, the last parameter collects `__VA_ARGS__`
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
        let stripped_body = engine::strip_comments(body.as_ref());
        let body_tokens = engine::tokenize_line(&stripped_body);
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

    /// Save the current definition of a macro onto its push stack (pragma push_macro).
    ///
    /// The saved entry is `None` if the macro is currently undefined.
    pub fn push_macro(&mut self, name: &str) {
        let saved = self.macros.get(name).cloned();
        self.macro_stack
            .entry(name.to_string())
            .or_default()
            .push(saved);
    }

    /// Restore the most recently saved definition of a macro (pragma pop_macro).
    ///
    /// If nothing was saved, or the saved state was "undefined", the macro is
    /// removed. No-op if `push_macro` was never called for `name`.
    pub fn pop_macro(&mut self, name: &str) {
        if let Some(stack) = self.macro_stack.get_mut(name)
            && let Some(saved) = stack.pop()
        {
            match saved {
                Some(mac) => {
                    self.macros.insert(name.to_string(), mac);
                }
                None => {
                    self.macros.remove(name);
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PreprocessorConfig;

    #[test]
    fn x86_64_configuration_defines_glibc_architecture_macros() {
        let config = PreprocessorConfig::for_linux().with_architecture(Architecture::X86_64);
        let driver = crate::PreprocessorDriver::with_config(&config);

        assert!(driver.is_defined("__x86_64__"));
        assert!(driver.is_defined("__LP64__"));
        assert!(!driver.is_defined("__i386__"));
    }

    #[test]
    fn x86_configuration_does_not_define_64_bit_macros() {
        let config = PreprocessorConfig::for_linux().with_architecture(Architecture::X86);
        let driver = crate::PreprocessorDriver::with_config(&config);

        assert!(driver.is_defined("__i386__"));
        assert!(!driver.is_defined("__x86_64__"));
        assert!(!driver.is_defined("__LP64__"));
    }
}
