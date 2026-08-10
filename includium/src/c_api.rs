use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::rc::Rc;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

use crate::config::{
    CStandard, Compiler, ExecutionEnvironment, LineEnding, PreprocessorConfig, Target,
};
use crate::driver::PreprocessorDriver;

/// Opaque C handle. Thin wrapper - all logic lives in `PreprocessorDriver`.
#[repr(C)]
pub struct includium_ctx(PreprocessorDriver);

/// C-friendly configuration struct for the preprocessor.
///
/// All fields must be set. Pass `0` for `recursion_limit` or values `> 10000`
/// to trigger a validation error.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct includium_config {
    /// Target OS: 0=Linux, 1=Windows, 2=MacOS
    pub target: c_int,
    /// Compiler: 0=GCC, 1=Clang, 2=MSVC
    pub compiler: c_int,
    /// Maximum macro recursion depth (must be 1..=10000)
    pub recursion_limit: usize,
    /// Number of include directories in `include_dirs`
    pub include_dirs_len: usize,
    /// Pointer to an array of `include_dirs_len` null-terminated include directory paths.
    /// May be null if `include_dirs_len` is 0.
    pub include_dirs: *const *const c_char,
    /// Warning handler callback (optional, can be null)
    pub warning_handler: Option<extern "C" fn(*const c_char)>,
}

/// Convenience type alias for [`includium_config`].
#[allow(non_camel_case_types)]
pub type includium_config_t = includium_config;

/// Set the last error message for C API error reporting
fn set_last_error(message: &str) {
    LAST_ERROR.with(|error| {
        *error.borrow_mut() = CString::new(message).ok();
    });
}

/// Convert C config to Rust config with validation
fn preprocessor_config_from_c(
    config: &includium_config_t,
) -> Result<PreprocessorConfig, &'static str> {
    let target = match config.target {
        0 => Target::Linux,
        1 => Target::Windows,
        2 => Target::MacOS,
        _ => return Err("Invalid target value"),
    };
    let compiler = match config.compiler {
        0 => Compiler::GCC,
        1 => Compiler::Clang,
        2 => Compiler::MSVC,
        _ => return Err("Invalid compiler value"),
    };
    if config.recursion_limit == 0 || config.recursion_limit > 10000 {
        return Err("Invalid recursion_limit");
    }
    let mut rust_config = PreprocessorConfig {
        target,
        compiler,
        standard: CStandard::C11,
        environment: ExecutionEnvironment::Hosted,
        recursion_limit: config.recursion_limit,
        include_resolver: None,
        include_dirs: Vec::new(),
        warning_handler: None,
        line_ending: LineEnding::LF,
    };

    // Collect include directories from the C array.
    if config.include_dirs_len > 0 && !config.include_dirs.is_null() {
        let dirs =
            unsafe { std::slice::from_raw_parts(config.include_dirs, config.include_dirs_len) };
        for &dir_ptr in dirs {
            if dir_ptr.is_null() {
                continue;
            }
            if let Ok(dir) = unsafe { CStr::from_ptr(dir_ptr).to_str() } {
                rust_config.include_dirs.push(dir.to_string());
            }
        }
    }
    if let Some(handler) = config.warning_handler {
        let handler_rc = Rc::new(move |msg: &str| {
            let Ok(c_msg) = CString::new(msg) else { return };
            handler(c_msg.as_ptr());
        });
        rust_config.warning_handler = Some(handler_rc);
    }
    Ok(rust_config)
}

/// Create a new preprocessor instance.
///
/// Returns a valid handle on success, or `NULL` if `config` validation fails
/// (check [`includium_last_error`] for the reason). Pass a null `config` to
/// use the default configuration (Linux, GCC, recursion limit 128).
///
/// # Safety
/// `config` must be either null or a valid pointer to an `includium_config`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_new(config: *const includium_config_t) -> *mut includium_ctx {
    let mut driver = PreprocessorDriver::new();
    if !config.is_null() {
        let c_config = unsafe { &*config };
        match preprocessor_config_from_c(c_config) {
            Ok(rust_config) => driver.apply_config(&rust_config),
            Err(e) => {
                set_last_error(e);
                return ptr::null_mut();
            }
        }
    }
    Box::into_raw(Box::new(includium_ctx(driver)))
}

/// Get the last error message from the C API.
///
/// Returns `NULL` when no error has occurred. The returned pointer is valid
/// until the next C API call that sets an error.
///
/// # Safety
/// The returned pointer must not be freed or modified by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_last_error() -> *const c_char {
    LAST_ERROR.with(|error| error.borrow().as_ref().map_or(ptr::null(), |s| s.as_ptr()))
}

/// Free a preprocessor instance created by [`includium_new`].
/// Passing a null pointer is a safe no-op.
///
/// # Safety
/// `ctx` must have been created by `includium_new` and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_free(ctx: *mut includium_ctx) {
    if !ctx.is_null() {
        unsafe {
            drop(Box::from_raw(ctx));
        }
    }
}

/// Process C source code and return the preprocessed result.
///
/// Returns a null-terminated string that must be freed with [`includium_free_result`],
/// or `NULL` on error (check [`includium_last_error`] for details). Returns `NULL`
/// silently if either `ctx` or `input` is null.
///
/// # Safety
/// - `ctx` must be a valid handle created by [`includium_new`]
/// - `input` must point to a valid null-terminated UTF-8 C string
/// - The returned string must be freed with [`includium_free_result`]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_process(
    ctx: *mut includium_ctx,
    input: *const c_char,
) -> *mut c_char {
    if ctx.is_null() || input.is_null() {
        return ptr::null_mut();
    }

    let Ok(input_str) = (unsafe { CStr::from_ptr(input).to_str() }) else {
        set_last_error("Invalid UTF-8 input");
        return ptr::null_mut();
    };
    let driver = unsafe { &mut (*ctx).0 };
    match driver.process(input_str) {
        Ok(result) => {
            if let Ok(cstr) = CString::new(result) {
                cstr.into_raw()
            } else {
                set_last_error("Result contains invalid UTF-8");
                ptr::null_mut()
            }
        }
        Err(e) => {
            set_last_error(&format!("Processing error: {e}"));
            ptr::null_mut()
        }
    }
}

/// Free a result string returned by [`includium_process`].
/// Passing a null pointer is a safe no-op.
///
/// # Safety
/// `result` must have been returned by `includium_process` and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_free_result(result: *mut c_char) {
    if !result.is_null() {
        unsafe {
            drop(CString::from_raw(result));
        }
    }
}
