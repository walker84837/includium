# Contributing to Includium

Thank you for your interest in contributing to Includium! This document outlines how to work on the project and what to expect when contributing.

## Prerequisites

- Rust **1.85.x or later** (latest stable recommended) — [https://rustup.rs/](https://rustup.rs/)
- Cargo (included with Rust)
- Git

Includium implements a C preprocessor, so familiarity with C/C++ preprocessing is helpful **when working on core logic**, especially:

- Object-like vs function-like macros
- Conditional directives (`#if`, `#ifdef`, `#elif`, …)
- `#include`, `#define`, `#undef`, pragmas, diagnostics
- Token pasting (`##`) and stringification (`#`)

## Getting Started

1. Fork and clone the repository:

   ```bash
   git clone https://github.com/yourusername/includium.git
   cd includium
   ```

2. Build the project:

   ```bash
   cargo build
   ```

3. Run tests:

   ```bash
   cargo test
   ```

## Code Style

We follow standard Rust conventions.

- Format code:

  ```bash
  cargo fmt
  ```

- Run Clippy and address warnings where reasonable:

  ```bash
  cargo clippy
  ```

- All public APIs must be [documented](#documentation). Public items without documentation will fail linting.

Example:

```rust
/// Process C code with the given configuration.
///
/// # Errors
///
/// Returns `PreprocessError` if the input contains malformed directives.
pub fn preprocess_c_code(
    input: &str,
    config: &PreprocessorConfig,
) -> Result<String, PreprocessError> {
    todo!()
}
```

### Security and Robustness

> [!WARNING]
> Includium processes untrusted input. Defensive coding matters.

- Never execute arbitrary code during preprocessing
- Be careful with recursion and macro expansion depth
- Sanitize and validate include paths

### Documentation

- Use Rustdoc comments for public APIs
- Document error conditions and non-obvious behavior
- Update the README for user-facing changes when needed

### Structure and Design

- Keep related functionality in the same module
- Minimize the public API surface
- Follow standard Rust module conventions

## Testing

Unit tests live alongside the code:

```rust
#[cfg(test)]
mod tests {
    // ...
}
```

When testing preprocessor behavior:

- Cover both valid and invalid cases
- Include edge cases
- Test different target configurations
- Keep tests deterministic

Example:

```rust
#[test]
fn test_function_like_macro() {
    let src = r#"
#define ADD(a, b) ((a)+(b))
int z = ADD(1, 2);
"#;
    let mut pp = Preprocessor::new();
    let out = pp.process(src).unwrap();
    assert!(out.contains("((1)+(2))"));
}
```

## Bug Reports

When reporting a bug:

- Provide a **minimal reproducible example**
- Show **expected vs actual output**
- Include:

  * Rust version (`rustc --version`)
  * Operating system
  * Target configuration

Example:

```c
#define FOO(x) x + 1
int y = FOO(2 * 3); // Expected: (2 * 3) + 1
```

## Performance

For performance-sensitive changes:

- Consider memory usage on large files
- Profile where appropriate (`perf`, `flamegraph`)
- Document performance implications of new behavior

## Architecture, Features, and Safety

> [!NOTE]
> This section applies if you are adding new functionality or touching core logic.

### Adding New Features

When introducing new preprocessor features:

- Prefer standards-compliant behavior (C11, C++17, etc.)
- Add comprehensive tests
- Document observable behavior and edge cases
- Consider backward compatibility and configurability

### Getting Help

If you're unsure about an approach or behavior:

- Open an issue with your question or proposal
- Check existing documentation and closed issues
- Ask early if a design decision might affect compatibility or correctness

## Thank You

Thanks for contributing to Includium! Every bug report, test, and patch helps improve the project for everyone.
