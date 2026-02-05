use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::rc::Rc;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = RefCell::new(None);
}

use crate::config::{Compiler, PreprocessorConfig, Target};
use crate::driver::PreprocessorDriver;

/// C-friendly configuration struct for the preprocessor
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct includium_config {
    /// Target OS: 0=Linux, 1=Windows, 2=MacOS
    pub target: c_int,
    /// Compiler: 0=GCC, 1=Clang, 2=MSVC
    pub compiler: c_int,
    /// Recursion limit
    pub recursion_limit: usize,
    /// Warning handler callback (optional, can be null)
    pub warning_handler: Option<extern "C" fn(*const c_char)>,
}

/// Typedef for includium_config
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
        recursion_limit: config.recursion_limit,
        include_resolver: None,
        warning_handler: None,
    };
    if let Some(handler) = config.warning_handler {
        let handler_rc = Rc::new(move |msg: &str| {
            let c_msg = match CString::new(msg) {
                Ok(s) => s,
                Err(_) => return,
            };
            handler(c_msg.as_ptr());
        });
        rust_config.warning_handler = Some(handler_rc);
    }
    Ok(rust_config)
}

/// Create a new preprocessor instance for C API
///
/// # Safety
/// This function is safe to call from C code.
/// If config is null, uses default configuration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_new(
    config: *const includium_config_t,
) -> *mut PreprocessorDriver {
    let mut driver = PreprocessorDriver::new();
    if !config.is_null() {
        let c_config = unsafe { &*config };
        match preprocessor_config_from_c(c_config) {
            Ok(rust_config) => driver.apply_config(&rust_config),
            Err(e) => {
                set_last_error(e);
                return ptr::null_mut(); // Invalid config
            }
        }
    }

    /// Get the last error message from the C API
    ///
    /// # Safety
    /// The returned string is valid until the next C API call that sets an error.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn includium_last_error() -> *const c_char {
        LAST_ERROR.with(|error| error.borrow().as_ref().map_or(ptr::null(), |s| s.as_ptr()))
    }
    Box::into_raw(Box::new(driver))
}

/// Free a preprocessor instance created by C API
///
/// # Safety
/// The pointer must have been created by `includium_new` and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_free(pp: *mut PreprocessorDriver) {
    if !pp.is_null() {
        unsafe {
            drop(Box::from_raw(pp));
        }
    }
}

/// Process C code and return the preprocessed result (C API)
///
/// # Safety
/// - The `pp` pointer must be valid and created by `includium_new`
/// - The `input` pointer must point to a valid null-terminated C string
/// - The returned string must be freed with `includium_free_result`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_process(
    pp: *mut PreprocessorDriver,
    input: *const c_char,
) -> *mut c_char {
    if pp.is_null() || input.is_null() {
        return ptr::null_mut();
    }

    let input_str = match unsafe { CStr::from_ptr(input).to_str() } {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 input");
            return ptr::null_mut(); // Invalid UTF-8 input
        }
    };
    let driver = unsafe { &mut *pp };
    match driver.process(input_str) {
        Ok(result) => match CString::new(result) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                set_last_error("Result contains invalid UTF-8");
                ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(&format!("Processing error: {}", e));
            ptr::null_mut()
        }
    }
}

/// Free a result string returned by C API
///
/// # Safety
/// The pointer must have been returned by `includium_process` and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn includium_free_result(result: *mut c_char) {
    if !result.is_null() {
        unsafe {
            drop(CString::from_raw(result));
        }
    }
}
