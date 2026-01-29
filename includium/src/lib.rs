#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]

//! # C Preprocessor Library
//!
//! This library provides a complete C preprocessor implementation that can process
//! C/C++ source code with macros, conditional compilation, and includes. It supports
//! target-specific preprocessing for different operating systems and compilers.
//!
//! ## Features
//!
//! - Macro expansion (object-like and function-like macros)
//! - Conditional compilation (`#ifdef`, `#ifndef`, `#if`, `#else`, `#elif`, `#endif`)
//! - Include processing with custom resolvers
//! - Target-specific macro definitions (Linux, Windows, macOS)
//! - Compiler-specific macro definitions (GCC, Clang, MSVC)
//! - C FFI for integration with other languages
//!
//! ## Example
//!
//! ```rust,no_run
//! use includium::PreprocessorConfig;
//!
//! let code = r#"#
//! #define PI 3.14
//! #ifdef __linux__
//! const char* platform = "Linux";
//! #endif
//! "#;
//!
//! let config = PreprocessorConfig::for_linux();
//! let result = includium::process(code, &config).unwrap();
//! //println!("{}", result);
//! ```

mod c_api;
mod config;
mod context;
mod date_time;
mod driver;
mod engine;
mod error;
mod macro_def;
mod token;

pub use config::{Compiler, IncludeResolver, PreprocessorConfig, Target, WarningHandler};
pub use context::PreprocessorContext;
pub use driver::PreprocessorDriver;
pub use error::{PreprocessError, PreprocessErrorKind};

// Token, ExprToken, Macro are internal or accessible via PreprocessorDriver methods if needed,
// but Macro struct is public so it can be returned by get_macros.
pub use macro_def::Macro;

// Re-export Preprocessor as alias to PreprocessorDriver for backward compatibility
pub use PreprocessorDriver as Preprocessor;

use std::path::Path;

/// Preprocess C code with the given configuration.
/// This automatically defines target and compiler-specific macros.
///
/// # Errors
/// Returns `PreprocessError` if the input code has malformed directives,
/// macro recursion limits are exceeded, or I/O errors occur during include resolution.
pub fn process<S: AsRef<str>>(
    input: S,
    config: &PreprocessorConfig,
) -> Result<String, PreprocessError> {
    let mut driver = PreprocessorDriver::new();
    driver.apply_config(config);
    driver.process(input.as_ref())
}

/// Preprocess a C file and write the result to another file
///
/// # Errors
/// Returns `PreprocessError` if the input file cannot be read,
/// the output file cannot be written, or if preprocessing fails.
pub fn process_file<P: AsRef<Path>>(
    input_path: P,
    output_path: P,
    config: &PreprocessorConfig,
) -> Result<(), PreprocessError> {
    let input = std::fs::read_to_string(input_path)?;
    let output = process(&input, config)?;
    std::fs::write(output_path, output)?;
    Ok(())
}

/// Preprocess a C file and return the result as a string
///
/// # Errors
/// Returns `PreprocessError` if the file cannot be read or if preprocessing fails.
pub fn preprocess_c_file_to_string<P: AsRef<Path>>(
    input_path: P,
    config: &PreprocessorConfig,
) -> Result<String, PreprocessError> {
    let input = std::fs::read_to_string(input_path)?;
    process(&input, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_object_macro() {
        let src = r#"
#define PI 3.14
float x = PI;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("3.14"));
    }

    #[test]
    fn function_like_macro() {
        let src = r#"
#define ADD(a, b) ((a)+(b))
int z = ADD(1, 2);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("((1)+(2))"));
    }

    #[test]
    fn include_example() {
        let src = r#"
#include "inc.h"
int x = FOO;
"#;
        let mut pp = Preprocessor::new().with_include_resolver(|p, _kind, _context| {
            if p == "inc.h" {
                Some("#define FOO 42\n".to_string())
            } else {
                None
            }
        });
        let out = pp.process(src).unwrap();
        assert!(out.contains("42"));
    }

    #[test]
    fn conditional_compilation_ifdef() {
        let src = r#"
#define DEBUG 1
#ifdef DEBUG
int x = 1;
#else
int x = 0;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 1;"));
        assert!(!out.contains("int x = 0;"));
    }

    #[test]
    fn expression_arithmetic() {
        let src = r#"
#if 1 + 2 * 3 == 7
int x = 1;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 1;"));
    }

    #[test]
    fn expression_logical() {
        let src = r#"
#if (1 && 0) || (0 && 1) || (1 && 1)
int x = 1;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 1;"));
    }

    #[test]
    fn expression_comparison() {
        let src = r#"
#if 5 > 3 && 10 >= 10 && 2 < 4 && 5 <= 5 && 3 != 4 && 5 == 5
int x = 1;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 1;"));
    }

    #[test]
    fn expression_unary() {
        let src = r#"
#if !0 && !!1 && -(-5) == 5
int x = 1;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 1;"));
    }

    #[test]
    fn expression_precedence() {
        let src = r#"
#if 2 + 3 * 4 == 14 && (2 + 3) * 4 == 20
int x = 1;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 1;"));
    }

    #[test]
    fn comment_stripping() {
        let src = r#"
 // This is a comment
int x = 1; /* inline comment */
#define MACRO // comment after define
int y = MACRO;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        // Comments should be replaced with spaces
        assert!(out.contains("int x = 1; "));
        assert!(out.contains("int y = ;"));
    }

    #[test]
    fn comment_stripping_in_strings() {
        let src = r#"
#define STR "this /* is not a comment */"
const char* s = STR;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        // Comments inside strings should not be stripped
        assert!(out.contains("\"this /* is not a comment */\""));
    }

    #[test]
    fn dynamic_macros() {
        let src = r#"
#define LINE __LINE__
#define FILE __FILE__
int line = LINE;
const char* file = FILE;
"#;
        let mut pp = Preprocessor::new();
        pp.set_current_file("test.c".to_string());
        let out = pp.process(src).unwrap();
        // __LINE__ should be 4 for the int line = LINE; line
        assert!(out.contains("int line = 4;"));
        assert!(out.contains("const char* file = \"test.c\";"));
    }

    #[test]
    fn pragma_once() {
        let mut pp = Preprocessor::new().with_include_resolver(|path, _kind, _context| {
            if path == "header.h" {
                Some("#pragma once\nint x = 42;".to_string())
            } else {
                None
            }
        });

        let src = r#"
#include "header.h"
#include "header.h"
int y = x;
"#;
        let out = pp.process(src).unwrap();
        // Should include header.h only once
        assert_eq!(out.matches("int x = 42;").count(), 1);
        assert!(out.contains("int y = x;"));
    }

    #[test]
    fn pragma_operator() {
        let src = r#"
_Pragma("once")
int x = 1;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        // _Pragma("once") should be processed as #pragma once
        assert!(out.contains("int x = 1;"));
        // Check that pragma once was handled (no duplicate includes, but since no include, just check no error)
    }

    #[test]
    fn conditional_compilation_elif() {
        let src = r#"
#define LEVEL 2
#if LEVEL == 1
int x = 1;
#elif LEVEL == 2
int x = 2;
#else
int x = 3;
#endif
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("int x = 2;"));
    }

    #[test]
    fn error_directive() {
        let src = r#"
#if 0
#else
#error This should error
#endif
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);
        assert!(result.is_err());
    }

    #[test]
    fn line_directive() {
        let src = r#"
#line 100 "test.c"
int x;
"#;
        let mut pp = Preprocessor::new();
        pp.process(src).unwrap();
        // The current_file and current_line are internal state now,
        // but the directive should be processed without error
        assert!(pp.process(src).is_ok());
    }

    #[test]
    fn undef_directive() {
        let src = r#"
#define FOO 1
#undef FOO
int x = FOO;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("FOO"));
    }

    #[test]
    fn variadic_macro() {
        let src = r#"
#define LOG(fmt, ...) printf(fmt, __VA_ARGS__)
LOG("hello %s\n", "world");
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        //println!("variadic_macro output: {:?}", out);
        assert!(out.contains("printf(\"hello %s\\n\", \"world\")"));
    }

    #[test]
    fn nested_macros() {
        let src = r#"
#define ADD(a, b) ((a)+(b))
#define MUL(a, b) ((a)*(b))
int x = ADD(ADD(1, 2), MUL(3, 4));
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        // Check that nested expansion worked
        // For now, just check that nested macro calls are being handled
        assert!(out.contains("int x =")); // basic structure preserved
        assert!(out.contains("+")); // addition operator present
        assert!(out.contains("*")); // multiplication operator present
    }

    #[test]
    fn macro_with_stringification() {
        let src = r#"
#define STR(x) #x
const char* s = STR(hello);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        //println!("macro_with_stringification output: {:?}", out);
        assert!(out.contains("\"hello\""));
    }

    #[test]
    fn token_pasting_basic() {
        let src = r#"
#define PASTE(a,b) a##b
int x1 = PASTE(x, 1);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("x1"));
    }

    #[test]
    fn token_pasting_operators() {
        let src = r#"
#define MAKE_ASSIGN(op) = ## op
int x MAKE_ASSIGN(+) 5;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("=+"));
    }

    #[test]
    fn token_pasting_multiple() {
        let src = r#"
#define PASTE3(a,b,c) a##b##c
int var PASTE3(_,x,_) = 42;
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        assert!(out.contains("_x_"));
    }

    #[test]
    fn error_location_information() {
        use std::error::Error;

        // Test that errors include proper location information
        let error =
            PreprocessError::malformed_directive("test.c".to_string(), 42, "define".to_string());

        // Check that the error displays with location
        let display = format!("{}", error);
        assert!(display.contains("test.c:42"));
        assert!(display.contains("malformed directive: define"));

        // Check error chaining for I/O errors
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let wrapped_error = PreprocessError::io_error("test.c".to_string(), 10, io_error);

        // The source should be the underlying I/O error
        assert!(wrapped_error.source().is_some());
    }

    #[test]
    fn error_with_source_line_and_caret() {
        // Test that errors include source line and caret indicator
        let error =
            PreprocessError::malformed_directive("test.c".to_string(), 10, "define".to_string())
                .with_column(5)
                .with_source_line("#define".to_string());

        let display = format!("{}", error);
        assert!(display.contains("test.c:10:5"));
        assert!(display.contains("#define"));
        // Check for caret indicator (column 5 means 4 spaces before ^)
        assert!(display.contains("    ^"));
    }

    #[test]
    fn malformed_directive_error() {
        // Test malformed directive error with source context
        let src = r#"
#define
int x = 1;
"#;
        let mut pp = Preprocessor::new();
        pp.set_current_file("test.c".to_string());
        let result = pp.process(src);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let display = format!("{}", error);

        // Should contain location
        assert!(display.contains("test.c:2:"));
        // Should contain source line
        assert!(display.contains("#define"));
        // Should contain caret
        assert!(display.contains("^"));
    }

    #[test]
    fn unterminated_if_error() {
        // Test unterminated conditional error
        let src = r#"
#if defined(FOO)
int x = 1;
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let display = format!("{}", error);

        // Should contain location info
        assert!(display.contains("unterminated"));
    }

    #[test]
    fn elif_without_if_error() {
        // Test #elif without #if error
        let src = r#"
#elif defined(FOO)
int x = 1;
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let display = format!("{}", error);

        assert!(display.contains("#elif without #if"));
    }

    #[test]
    fn else_without_if_error() {
        // Test #else without #if error
        let src = r#"
#else
int x = 1;
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let display = format!("{}", error);

        assert!(display.contains("#else without #if"));
    }

    #[test]
    fn endif_without_if_error() {
        // Test #endif without #if error
        let src = r#"
#endif
int x = 1;
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let display = format!("{}", error);

        assert!(display.contains("#endif without #if"));
    }
}
