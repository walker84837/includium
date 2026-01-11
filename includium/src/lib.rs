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
//! use includium::{preprocess_c_code, PreprocessorConfig};
//!
//! let code = r#"#
//! #define PI 3.14
//! #ifdef __linux__
//! const char* platform = "Linux";
//! #endif
//! "#;
//!
//! let config = PreprocessorConfig::for_linux();
//! let result = preprocess_c_code(code, &config).unwrap();
//! println!("{}", result);
//! ```

mod c_api;
mod config;
mod error;
mod macro_def;
mod preprocessor;
mod token;

pub use config::{Compiler, IncludeResolver, PreprocessorConfig, Target};
pub use error::PreprocessError;
pub use preprocessor::Preprocessor;

// Token, ExprToken, Macro are internal or accessible via Preprocessor methods if needed,
// but Macro struct is public so it can be returned by get_macros.
pub use macro_def::Macro;

use std::path::Path;

/// Preprocess C code with the given configuration.
/// This automatically defines target and compiler-specific macros.
///
/// # Errors
/// Returns `PreprocessError` if the input code has malformed directives,
/// macro recursion limits are exceeded, or I/O errors occur during include resolution.
pub fn preprocess_c_code<S: AsRef<str>>(
    input: S,
    config: &PreprocessorConfig,
) -> Result<String, PreprocessError> {
    let mut preprocessor = Preprocessor::new();
    preprocessor.apply_config(config);
    preprocessor.process(input.as_ref())
}

/// Preprocess a C file and write the result to another file
///
/// # Errors
/// Returns `PreprocessError` if the input file cannot be read,
/// the output file cannot be written, or if preprocessing fails.
pub fn preprocess_c_file<P: AsRef<Path>>(
    input_path: P,
    output_path: P,
    config: &PreprocessorConfig,
) -> Result<(), PreprocessError> {
    let input = std::fs::read_to_string(input_path)?;
    let output = preprocess_c_code(&input, config)?;
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
    preprocess_c_code(&input, config)
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
        let mut pp = Preprocessor::new().with_include_resolver(|p| {
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
    fn dynamic_macros() {
        let src = r#"
#define LINE __LINE__
#define FILE __FILE__
int line = LINE;
const char* file = FILE;
"#;
        let mut pp = Preprocessor::new();
        pp.current_file = "test.c".to_string();
        let out = pp.process(src).unwrap();
        // __LINE__ should be 4 for the int line = LINE; line
        assert!(out.contains("int line = 4;"));
        assert!(out.contains("const char* file = \"test.c\";"));
    }

    #[test]
    fn pragma_once() {
        let mut pp = Preprocessor::new();
        pp.include_resolver = Some(std::rc::Rc::new(|path: &str| {
            if path == "header.h" {
                Some("#pragma once\nint x = 42;".to_string())
            } else {
                None
            }
        }));

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
        assert_eq!(pp.current_line, 101);
        assert_eq!(pp.current_file, "test.c");
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
        // Check that nested expansion worked: ADD(ADD(1,2), MUL(3,4)) should expand to ((ADD(1,2))+(MUL(3,4)))
        // which should further expand to (((1)+(2))+((3)*(4)))
        assert!(out.contains("((1)+(2))")); // inner ADD expanded
        assert!(out.contains("((3)*(4))")); // MUL expanded
        assert!(out.contains("int x =")); // basic structure preserved
    }

    #[test]
    fn macro_with_stringification() {
        let src = r#"
#define STR(x) #x
const char* s = STR(hello);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
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
}
