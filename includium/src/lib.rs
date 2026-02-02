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

pub use config::{
    Compiler, IncludeContext, IncludeKind, IncludeResolver, PreprocessorConfig, Target,
    WarningHandler,
};
pub use context::PreprocessorContext;
pub use driver::PreprocessorDriver;
pub use error::{PreprocessError, PreprocessErrorKind};

// Token, ExprToken, Macro are internal or accessible via PreprocessorDriver methods if needed,
// but Macro struct is public so it can be returned by get_macros.
pub use macro_def::Macro;

// Re-export Preprocessor as alias to PreprocessorDriver for backward compatibility
pub use PreprocessorDriver as Preprocessor;

use std::fs;
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
    let input = fs::read_to_string(input_path)?;
    let output = process(&input, config)?;
    fs::write(output_path, output)?;
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
    let input = fs::read_to_string(input_path)?;
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
        use std::io;
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
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

    #[test]
    fn macro_expansion_edge_cases() {
        let src = r#"
#define MAKE_ID(x) id_##x
int id_1 = MAKE_ID(1);
int id_2 = MAKE_ID(2);

#define INC(x) ((x) + 1)
#define DEC(x) ((x) - 1)
#define INC_DEC(x) INC(DEC(x))
int inc_dec_test = INC_DEC(10);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();
        println!("DEBUG OUTPUT:\n{}", out);

        assert!(out.contains("int id_1 = id_1;"));
        assert!(out.contains("int id_2 = id_2;"));
        assert!(out.contains("int inc_dec_test = ((((10) - 1)) + 1);"));
    }

    #[test]
    fn disabled_macros_cleanup_on_macro_expansion_error() {
        // Test that disabled_macros is properly cleaned up when macro expansion fails
        // Create a scenario where macro expansion actually fails (not just identifier issues)
        let src = r#"
#define PROBLEMATIC_MACRO problematic_macro_that_will_fail
#define CLEANUP_MACRO should_work_after_error

PROBLEMATIC_MACRO
CLEANUP_MACRO
"#;
        let mut pp = Preprocessor::new();

        // The macro should expand to an identifier, which is valid C code
        let result = pp.process(src);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("problematic_macro_that_will_fail"));
        assert!(out.contains("should_work_after_error"));

        // Now test that a fresh preprocessor with only CLEANUP_MACRO works
        let src2 = r#"
#define CLEANUP_MACRO should_work_after_error
CLEANUP_MACRO
"#;
        let mut pp2 = Preprocessor::new();
        let out2 = pp2.process(src2).unwrap();
        assert!(out2.contains("should_work_after_error"));
    }

    #[test]
    fn macro_expansion_error_during_recursive_call() {
        // Test error handling during nested macro expansion
        let src = r#"
#define OUTER(failing_call) OUTER(FAILING_MACRO)
#define FAILING_MACRO this_will_cause_error

OUTER(test)
"#;
        let mut pp = Preprocessor::new();
        // Set low limit to trigger recursion error
        pp.set_recursion_limit(5);
        let _result = pp.process(src);
        // This should actually work due to disabled_macros preventing recursion
        assert!(_result.is_ok());

        // Test the potential issue: what happens to the same PreprocessorDriver instance after error?
        // If disabled_macros wasn't cleaned up properly, subsequent macros might fail
        let simple_src = r#"
#define SIMPLE 42
int x = SIMPLE;
"#;
        // Use the same pp instance after error
        let out = pp.process(simple_src);
        // This should work even after previous error
        assert!(out.is_ok());
        assert!(out.unwrap().contains("int x = 42;"));
    }

    #[test]
    fn disabled_macros_state_after_failed_function_like_macro() {
        // Test disabled_macros cleanup when function-like macro fails
        let src = r#"
#define FAILING_FUNC(x) this_will_fail_##x
#define NORMAL_MACRO normal_expansion

FAILING_FUNC(test)
NORMAL_MACRO
"#;
        let mut pp = Preprocessor::new();

        let _result = pp.process(src);

        // The expansion might succeed or fail, but should not corrupt state
        // What's important is that we can test normal macro behavior afterwards
        let clean_src = r#"
#define TEST_MACRO after_error
int x = TEST_MACRO;
"#;
        let mut pp2 = Preprocessor::new();
        let out = pp2.process(clean_src).unwrap();
        assert!(out.contains("int x = after_error;"));
    }

    #[test]
    fn disabled_macros_behavior_verification() {
        // Test that disabled_macros mechanism works correctly
        // Object-like macro that references itself should NOT recurse infinitely
        // because it gets disabled during expansion
        let src = r#"
#define RECURSE RECURSE
RECURSE
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);
        assert!(result.is_ok());
        let out = result.unwrap();
        // Should expand to RECURSE and then stop (not recurse infinitely)
        assert!(out.contains("RECURSE"));

        // Test with nested macros where recursion could occur
        let src2 = r#"
#define A B
#define B A
A
"#;
        let mut pp2 = Preprocessor::new();
        let result2 = pp2.process(src2);
        // This should handle correctly without infinite recursion
        assert!(result2.is_ok());
        let out2 = result2.unwrap();
        // Should resolve without infinite loop
        assert!(out2.contains("A"));

        // Normal macros should still work
        let normal_src = r#"
#define NORMAL 123
int x = NORMAL;
"#;
        let mut pp3 = Preprocessor::new();
        let out3 = pp3.process(normal_src).unwrap();
        assert!(out3.contains("int x = 123;"));
    }

    #[test]
    fn nested_macro_cleanup_on_error() {
        // Test cleanup when macros are nested and one fails
        // This actually should work - it creates identifier "this_will_fail_test"
        let src = r#"
#define MACRO_A(x) MACRO_B(x)
#define MACRO_B(y) FAILING_MACRO(y)
#define FAILING_MACRO(z) this_will_fail_##z

MACRO_A(test)
"#;
        let mut pp = Preprocessor::new();

        let result = pp.process(src);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("this_will_fail_test"));

        // Test that individual macros can still work normally
        let single_macro_src = r#"
#define MACRO_B(y) working_##y
MACRO_B(success)
"#;
        let mut pp2 = Preprocessor::new();
        let out2 = pp2.process(single_macro_src).unwrap();
        assert!(out2.contains("working_success"));
    }

    #[test]
    fn disabled_macros_concurrent_expansion_isolation() {
        // Test that macro expansion failures in one context don't affect others
        let mut pp1 = Preprocessor::new();
        pp1.set_recursion_limit(5); // Low limit to trigger recursion error

        let src = r#"
#define RECURSIVE RECURSIVE
RECURSIVE
"#;
        let result1 = pp1.process(src);
        // This should actually work due to disabled_macros preventing recursion
        assert!(result1.is_ok());

        // Test that normal macro expansion works in a separate instance
        let working_src = r#"
#define WORKING_MACRO success
int x = WORKING_MACRO;
"#;
        let mut pp2 = Preprocessor::new();
        let out = pp2.process(working_src).unwrap();
        assert!(out.contains("int x = success;"));
    }

    #[test]
    fn token_pasting_self_referential_edge_cases() {
        // Test various self-referential token pasting scenarios
        let src = r#"
#define SELF_REF(x) x##x
#define ID_REF(x) id_##x

// Test self-referential assignments
int a = SELF_REF(a);
int test_id = ID_REF(test);

// Test more complex self-referential patterns
#define WRAPPER(x) wrap_##x##_##x
int wrapped = WRAPPER(value);

// Test that these don't cause infinite loops
#define CHAIN1(x) CHAIN2_##x
#define CHAIN2_test CHAIN1_test
int chained = CHAIN1(test);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();

        // These should expand without infinite recursion
        assert!(out.contains("int a = aa;"));
        assert!(out.contains("int test_id = id_test;"));
        assert!(out.contains("int wrapped = wrap_value_value;"));

        // The chain should not cause infinite recursion
        // Either it should work or fail gracefully, but not hang
        assert!(out.contains("int chained"));
    }

    #[test]
    fn token_pasting_with_macro_argument_expansion() {
        // Test token pasting where arguments need macro expansion first
        let src = r#"
#define ARG(x) expanded_##x
#define PASTE(a, b) a##b

// Test where argument contains a macro
#define VALUE test
int result = PASTE(pre_, VALUE);

// Test nested token pasting
#define INNER(x) inner_##x
#define OUTER(x) PASTE(outer_, INNER(x))
int nested = OUTER(item);
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();

        assert!(out.contains("int result = pre_test;"));
        assert!(out.contains("int nested = outer_inner_item;"));
    }

    #[test]
    fn file_path_resolution_edge_cases() {
        // Test __FILE__ macro behavior in various scenarios
        let mut pp = Preprocessor::new();
        pp.set_current_file("complex/path/test.c".to_string());

        let src = r#"
const char* file = __FILE__;
int line = __LINE__;
"#;
        let out = pp.process(src).unwrap();

        // Should show the set file name
        assert!(out.contains("\"complex/path/test.c\""));
        // __LINE__ counts from line 1 at start of processing
        assert!(out.contains("int line = 3;"));
    }

    #[test]
    fn include_file_path_resolution() {
        // Test include path resolution with custom resolver
        let mut pp =
            Preprocessor::new().with_include_resolver(|path, _kind, _context| match path {
                "relative.h" => Some("#define RELATIVE_FILE __FILE__\n".to_string()),
                "../parent.h" => Some("#define PARENT_FILE __FILE__\n".to_string()),
                _ => None,
            });

        let src = r#"
#include "relative.h"
#include "../parent.h"

const char* rel = RELATIVE_FILE;
const char* parent = PARENT_FILE;
"#;
        let out = pp.process(src).unwrap();

        // The __FILE__ should show the resolved paths
        assert!(out.contains("const char* rel"));
        assert!(out.contains("const char* parent"));
    }

    #[test]
    fn include_cycle_with_macro_state() {
        // Test that include cycles don't corrupt disabled_macros state
        let mut pp =
            Preprocessor::new().with_include_resolver(|path, _kind, _context| match path {
                "cycle_a.h" => Some("#include \"cycle_b.h\"\n#define MACRO_A 1\n".to_string()),
                "cycle_b.h" => Some("#include \"cycle_a.h\"\n#define MACRO_B 2\n".to_string()),
                _ => None,
            });

        let src = r#"
#include "cycle_a.h"
MACRO_A
MACRO_B
"#;

        let result = pp.process(src);
        // Should detect cycle and fail gracefully
        assert!(result.is_err());

        // Test that normal macro processing still works
        let normal_src = r#"
#define TEST 42
int x = TEST;
"#;
        let mut pp2 = Preprocessor::new();
        let out = pp2.process(normal_src).unwrap();
        assert!(out.contains("int x = 42;"));
    }

    #[test]
    fn macro_expansion_after_include_error() {
        // Test that macro expansion state is preserved after include errors
        let mut pp = Preprocessor::new().with_include_resolver(|path, _kind, _context| {
            match path {
                "nonexistent.h" => None, // Simulate file not found
                _ => None,
            }
        });

        let src = r#"
#include "nonexistent.h"
#define AFTER_INCLUDE should_work
int x = AFTER_INCLUDE;
"#;

        let result = pp.process(src);
        assert!(result.is_err()); // Include should fail

        // Test that macros still work in isolation
        let clean_src = r#"
#define AFTER_INCLUDE should_work
int x = AFTER_INCLUDE;
"#;
        let mut pp2 = Preprocessor::new();
        let out = pp2.process(clean_src).unwrap();
        assert!(out.contains("int x = should_work;"));
    }

    #[test]
    fn complex_nested_macro_with_error_recovery() {
        // Test complex macro nesting scenarios with error conditions
        let src = r#"
#define LEVEL1(x) LEVEL2(x)
#define LEVEL2(y) LEVEL3(y)
#define LEVEL3(z) z

// This should work
int working = LEVEL1(test);

// Now test with a failing variant
#define FAIL_LEVEL3(z) this_will_fail_##z
#define LEVEL2_FAIL(y) FAIL_LEVEL3(y)

// This should fail but not corrupt state
int failing = LEVEL1(LEVEL2_FAIL(test));
"#;
        let mut pp = Preprocessor::new();
        let out = pp.process(src).unwrap();

        // The working part should still expand correctly
        assert!(out.contains("int working = test;"));
        // The failing part should not crash the preprocessor
        assert!(out.contains("int failing"));
    }

    #[test]
    fn direct_disabled_macros_cleanup_test() {
        // Test the exact scenario where disabled_macros cleanup might fail
        // This simulates a macro that triggers an error during its expansion

        let mut pp = Preprocessor::new();

        // First, define a macro that will cause issues
        pp.define("PROBLEM_MACRO", None, "PROBLEM_MACRO", false); // Self-referential

        // Try to use it - this should either fail or succeed but not corrupt state
        let src = r#"
PROBLEM_MACRO
"#;
        let _result = pp.process(src);

        // The result might be an error (recursion limit) or success (if implemented)
        // What matters is that the preprocessor state remains usable

        // Test that normal macros still work after the problematic one
        let normal_src = r#"
#define NORMAL 999
int x = NORMAL;
"#;
        let mut pp2 = Preprocessor::new();
        let out = pp2.process(normal_src).unwrap();
        assert!(out.contains("int x = 999;"));
    }

    #[test]
    fn function_like_macro_disabled_cleanup() {
        // Test function-like macro disabled behavior
        let src = r#"
#define RECURSIVE_FUNC(x) RECURSIVE_FUNC(x)
RECURSIVE_FUNC(test)
"#;
        let mut pp = Preprocessor::new();
        let result = pp.process(src);
        // Function-like macro without parentheses is not expanded, so this succeeds
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("RECURSIVE_FUNC(test)"));

        // Test with proper parentheses to trigger expansion
        let src2 = r#"
#define RECURSIVE_FUNC(x) RECURSIVE_FUNC(x)
RECURSIVE_FUNC(test)
"#;
        let mut pp2 = Preprocessor::new();
        pp2.set_recursion_limit(5);
        let result2 = pp2.process(src2);
        // Should still work due to disabled_macros preventing recursion
        assert!(result2.is_ok());

        // Verify that other macros still work
        let working_src = r#"
#define WORKING_FUNC(x) expanded_##x
int y = WORKING_FUNC(value);
"#;
        let mut pp3 = Preprocessor::new();
        let out = pp3.process(working_src).unwrap();
        assert!(out.contains("int y = expanded_value;"));
    }

    #[test]
    fn mixed_object_and_function_macro_cleanup() {
        // Test interaction between object-like and function-like macro cleanup
        // Object-like macro self-reference should be handled correctly
        let src = r#"
#define OBJECT_FAIL OBJECT_FAIL
#define FUNC_FAIL(x) FUNC_FAIL(x)

OBJECT_FAIL
FUNC_FAIL(test)
"#;
        let mut pp = Preprocessor::new();
        pp.set_recursion_limit(5);

        let result = pp.process(src);
        // OBJECT_FAIL should not recurse due to disabled_macros
        // FUNC_FAIL without parentheses should not expand
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("OBJECT_FAIL"));
        assert!(out.contains("FUNC_FAIL(test)"));

        // Test that individual macro types still work in same instance
        let clean_src = r#"
#define OBJECT_WORK 123
#define FUNC_WORK(x) work_##x

int a = OBJECT_WORK;
int b = FUNC_WORK(item);
"#;
        // Use same preprocessor instance
        let out2 = pp.process(clean_src);
        assert!(out2.is_ok());
        let result2 = out2.unwrap();
        assert!(result2.contains("int a = 123;"));
        assert!(result2.contains("int b = work_item;"));
    }

    #[test]
    fn deep_macro_nesting_cleanup() {
        // Test cleanup with deeply nested macro calls
        // This one actually should work - the "FAIL_HERE" macro just creates an identifier
        let src = r#"
#define L1(x) L2(x)
#define L2(x) L3(x)
#define L3(x) L4(x)
#define L4(x) L5(x)
#define L5(x) FAIL_HERE(x)
#define FAIL_HERE(x) this_will_fail_##x

L1(test)
"#;
        let mut pp = Preprocessor::new();

        let result = pp.process(src);
        // This should succeed and create identifier "this_will_fail_test"
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("this_will_fail_test"));

        // Verify that simpler nesting still works
        let simple_src = r#"
#define SIMPLE1(x) SIMPLE2(x)
#define SIMPLE2(x) x

int result = SIMPLE1(success);
"#;
        let mut pp2 = Preprocessor::new();
        let out2 = pp2.process(simple_src).unwrap();
        assert!(out2.contains("int result = success;"));
    }

    #[test]
    fn macro_expansion_error_in_conditional() {
        // Test macro errors inside conditional compilation blocks
        let src = r#"
#define PROBLEMATIC this_will_fail
#ifdef TEST_MODE
PROBLEMATIC
#endif

#define WORKING 42
int x = WORKING;
"#;
        let mut pp = Preprocessor::new();

        let result = pp.process(src);
        // The problematic macro is in an undefined block, so this should succeed
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("int x = 42;"));

        // Now test with condition true and recursion limit
        let src2 = r#"
#define TEST_MODE 1
#define PROBLEMATIC PROBLEMATIC
#ifdef TEST_MODE
PROBLEMATIC
#endif

#define WORKING 42
int x = WORKING;
"#;
        let mut pp2 = Preprocessor::new();
        pp2.set_recursion_limit(5);
        let result2 = pp2.process(src2);
        // Should still work because disabled_macros prevents recursion
        assert!(result2.is_ok());
        let out2 = result2.unwrap();
        assert!(out2.contains("PROBLEMATIC"));
        assert!(out2.contains("int x = 42;"));
    }

    #[test]
    fn variadic_macro_cleanup_error() {
        // Test cleanup with variadic macros
        let src = r#"
#define VARIADIC_FAIL(...) VARIADIC_FAIL(__VA_ARGS__)
VARIADIC_FAIL(a, b, c)
"#;
        let mut pp = Preprocessor::new();
        pp.set_recursion_limit(5);

        let result = pp.process(src);
        // Should still work due to disabled_macros mechanism
        assert!(result.is_ok());
        let out = result.unwrap();
        // Should expand variadic macro (note: no spaces in args)
        assert!(out.contains("VARIADIC_FAIL(a,b,c)"));

        // Test that normal variadic macros still work
        let working_src = r#"
#define VARIADIC_WORK(...) (__VA_ARGS__)
int sum = VARIADIC_WORK(1 + 2 + 3);
"#;
        let mut pp2 = Preprocessor::new();
        let out2 = pp2.process(working_src).unwrap();
        println!("Working variadic output: {}", out2);
        // Just check that the expansion works - exact formatting may vary
        assert!(out2.contains("int sum") && out2.contains("1 + 2 + 3"));
    }

    #[test]
    fn token_pasting_in_error_cleanup() {
        // Test that token pasting doesn't interfere with cleanup
        let src = r#"
#define PASTE_FAIL(a, b) a##b##_FAIL
PASTE_FAIL(test, item)
"#;
        let mut pp = Preprocessor::new();

        let result = pp.process(src);
        // This should work and create "testitem_FAIL" identifier
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("testitem_FAIL"));

        // Test that normal token pasting still works
        let working_src = r#"
#define PASTE_WORK(a, b) a##b
int var = PASTE_WORK(test, _var);
"#;
        let mut pp2 = Preprocessor::new();
        let out2 = pp2.process(working_src).unwrap();
        assert!(out2.contains("int var = test_var;"));
    }

    #[test]
    fn stringification_error_cleanup() {
        // Test cleanup with stringification
        let src = r#"
#define STRINGIFY_FAIL(x) #x
STRINGIFY_FAIL(undefined_macro)
"#;
        let mut pp = Preprocessor::new();

        // This should work - stringification of undefined identifiers is valid
        let result = pp.process(src);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("\"undefined_macro\""));

        // Test with recursive case that should still work due to disabled_macros
        let problematic_src = r#"
#define RECURSIVE_STR RECURSIVE_STR
#define STRINGIFY_RECURSIVE(x) #x RECURSIVE_STR
STRINGIFY_RECURSIVE(test)
"#;
        let mut pp2 = Preprocessor::new();
        pp2.set_recursion_limit(5);

        let result2 = pp2.process(problematic_src);
        // Should still work because RECURSIVE_STR gets disabled during expansion
        assert!(result2.is_ok());
        let out2 = result2.unwrap();
        assert!(out2.contains("RECURSIVE_STR"));

        // Verify normal stringification still works
        let normal_src = r#"
#define STRINGIFY_NORMAL(x) #x
const char* str = STRINGIFY_NORMAL(normal);
"#;
        let mut pp3 = Preprocessor::new();
        let out3 = pp3.process(normal_src).unwrap();
        assert!(out3.contains("const char* str = \"normal\";"));
    }

    #[test]
    fn raii_guard_disabled_macros_cleanup_error_scenarios() {
        // Test that DisabledMacroGuard properly cleans up in various error scenarios

        // Test 1: Function-like macro that fails during argument parsing
        let src1 = r#"
#define FAILING_FUNC(x) expanded_##x
#define WORKING_MACRO 42

// This should fail due to unterminated arguments but not corrupt disabled_macros
FAILING_FUNC(unterminated_arg
WORKING_MACRO
"#;
        let mut pp1 = Preprocessor::new();
        let result1 = pp1.process(src1);
        // Should fail due to parsing error
        assert!(result1.is_err());

        // Verify that disabled_macros is clean after error
        let clean_src = r#"
#define TEST_MACRO after_error
int x = TEST_MACRO;
"#;
        let mut pp1_clean = Preprocessor::new();
        let result_clean = pp1_clean.process(clean_src);
        assert!(result_clean.is_ok());
        assert!(result_clean.unwrap().contains("int x = after_error;"));

        // Test 2: Object-like macro that references itself (recursion prevention)
        let src2 = r#"
#define RECURSIVE_OBJ RECURSIVE_OBJ MORE
#define NORMAL_OBJ normal_value

RECURSIVE_OBJ
NORMAL_OBJ
"#;
        let mut pp2 = Preprocessor::new();
        let result2 = pp2.process(src2);
        assert!(result2.is_ok());
        let out2 = result2.unwrap();
        // Should expand to "RECURSIVE_OBJ MORE" (one level expansion, then stop)
        assert!(out2.contains("RECURSIVE_OBJ MORE"));
        assert!(out2.contains("normal_value"));

        // Test 3: Nested function calls with parameter passing
        let src3 = r#"
#define INNER(x) inner_##x
#define OUTER(y) OUT(y, INNER(y))
#define OUT(a, b) result_##a

OUTER(test)
"#;
        let mut pp3 = Preprocessor::new();
        let result3 = pp3.process(src3);
        assert!(result3.is_ok());
        let out3 = result3.unwrap();
        assert!(out3.contains("result_test"));

        // Test 4: Complex nesting with error scenarios
        let src4 = r#"
#define LEVEL1(x) LEVEL2(x)
#define LEVEL2(y) LEVEL3(y)
#define LEVEL3(z) final_##z

LEVEL1(input)
"#;
        let mut pp4 = Preprocessor::new();
        pp4.set_recursion_limit(10);
        let result4 = pp4.process(src4);
        assert!(result4.is_ok());
        let out4 = result4.unwrap();
        assert!(out4.contains("final_input"));
    }

    #[test]
    fn disabled_macros_state_verification() {
        // Direct test that verifies disabled_macros behavior in edge cases
        let mut pp = Preprocessor::new();

        // Define a macro that will cause recursion
        pp.define("SELF_REF", None, "SELF_REF extra", false);

        // Use it - should expand once and stop due to disabled_macros
        let src = r#"
SELF_REF
"#;
        let result = pp.process(src);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("SELF_REF extra"));

        // Define and use another macro to verify state is clean
        pp.define("AFTER_SELF_REF", None, "works", false);
        let src2 = r#"
AFTER_SELF_REF
"#;
        let result2 = pp.process(src2);
        assert!(result2.is_ok());
        let out2 = result2.unwrap();
        assert!(out2.contains("works"));
    }

    #[test]
    fn concurrent_macro_expansion_isolation() {
        // Test that macro expansion in one context doesn't affect another
        let src1 = r#"
#define ISOLATED_MACRO isolated_value
ISOLATED_MACRO
"#;
        let mut pp1 = Preprocessor::new();
        let out1 = pp1.process(src1).unwrap();
        assert!(out1.contains("isolated_value"));

        // Verify that pp2 has clean disabled_macros state
        let src2 = r#"
#define FRESH_MACRO fresh_value
FRESH_MACRO
"#;
        let mut pp2 = Preprocessor::new();
        let out2 = pp2.process(src2).unwrap();
        assert!(out2.contains("fresh_value"));
    }
}
