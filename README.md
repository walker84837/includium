# Includium

[![Crates.io](https://img.shields.io/crates/v/includium)](https://crates.io/crates/includium)
[![Documentation](https://docs.rs/includium/badge.svg)](https://docs.rs/includium)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](LICENSE)

> A complete C preprocessor implementation in Rust.

[API Documentation](https://docs.rs/includium) | [CLI Documentation](includium-cli/) | [Changelog](CHANGELOG.md)

`includium` is a robust, well-tested C preprocessor that can process C/C++ source code with macros, conditional compilation, and includes. It supports target-specific preprocessing for different operating systems and compilers.

This repository contains both the core library and a command-line interface (`includium-cli`).

## Features

| Feature | Description |
|:--------|:------------|
| **Macro Expansion** | Supports both object-like and function-like macros |
| **Conditional Compilation** | Full support for `#ifdef`, `#ifndef`, `#if`, `#else`, `#elif`, `#endif` |
| **Include Processing** | Handles file inclusion with custom resolvers |
| **Target-Specific Definitions** | Pre-configured macros for Linux, Windows, and macOS |
| **Compiler Support** | Mock definitions for GCC, Clang, and MSVC |
| **C FFI** | Integration capabilities for use with other languages and ecosystems |

### Supported Platforms

| OS | Compilers | Status |
|:---|:----------|:-------|
| Linux | GCC, Clang | ✅ Fully supported |
| Windows | MSVC | ✅ Fully supported |
| macOS | Clang | ✅ Fully supported |

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
  - [As a Library](#as-a-library)
  - [As a CLI](#as-a-cli)
- [Development](#development)
  - [Just Tasks](#just-tasks)
- [Building from Source](#building-from-source)
- [Advanced Usage](#advanced-usage)
  - [Custom Include Resolvers](#custom-include-resolvers)
  - [`#warning` Handling](#warning-handling)
- [Contributing](#contributing)
  - [Roadmap](#roadmap)
  - [Reporting Issues](#reporting-issues)
- [License](#license)

## Installation

Add `includium` to your `Cargo.toml` using
```bash
cargo add includium
```
or manually by editing Cargo.toml:
```toml
[dependencies]
includium = "0.1.0"
```

## Usage

### As a Library

> [!WARNING]
> Compiler-specific macro definitions are **approximations**.
> While they match common behavior for GCC, Clang, and MSVC, edge cases may differ
> from real toolchains.

```rust
use includium::{preprocess_c_code, PreprocessorConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let code = r#"
#define PI 3.14
#ifdef __linux__
const char* platform = "Linux";
#endif
    "#;

    let config = PreprocessorConfig::for_linux();
    let result = preprocess_c_code(code, &config)?;
    println!("{}", result);
    Ok(())
}
```

### As a CLI

The `includium-cli` package provides a command-line interface for the C preprocessor. See the [CLI documentation](includium-cli/) for detailed usage instructions.

Basic installation:
```bash
cargo install includium-cli
```

Basic usage:
```bash
# Preprocess a file
includium input.c -o output.i

# Cross-compile for Windows
includium input.c --target windows --compiler msvc -o win_output.i
```

## Development

### Just Tasks

This project uses [Just](https://github.com/casey/just) for task automation. Available tasks:

|Task|Description|Arguments|
|:---|:----------|:--------|
|`build`|Build the workspace (library + CLI)|None|
|`build-release`|Build in release mode|None|
|`test`|Run all tests|None|
|`check`|Run all checks (format, lint, test)|None|
|`run`|Run the CLI with arguments|`--help`|
|`run-release`|Run the CLI in release mode|`input.c -o output.i`|
|`install`|Install the CLI locally|None|
|`clean`|Clean build artifacts|None|
|`docs`|Generate documentation|None|

## Building from Source

1. Clone the repository:
   ```bash
   git clone https://github.com/walker84837/includium.git
   cd includium
   ```

2. Build the workspace:
   ```bash
   cargo build # or just build
   ```

3. Run tests:
   ```bash
   cargo test # or just test
   ```

## Advanced Usage

> [!NOTE]
> This section is intended for advanced users who need custom include resolution
> or fine-grained control over diagnostics and preprocessing behavior.

### Custom Include Resolvers

```rust
use includium::Preprocessor;

let mut preprocessor = Preprocessor::new()
    .with_include_resolver(|path| {
        match path {
            "config.h" => Some("#define CONFIG_ENABLED 1".to_string()),
            "version.h" => Some("#define VERSION \"1.0.0\"".to_string()),
            _ => None,
        }
    });
```

### `#warning` Handling

```rust
use includium::PreprocessorConfig;
use std::rc::Rc;

let warning_handler = Rc::new(|msg: &str| {
    eprintln!("Warning: {}", msg);
});

let config = PreprocessorConfig::for_linux()
    .with_warning_handler(warning_handler);
```

## Contributing

We welcome contributions! Please see our [Contributing Guidelines](CONTRIBUTING.md) for details.

### Roadmap

- [x] CLI (includium-cli)
- [ ] Integration tests
- [ ] Benchmarks with `cargo bench`
- [ ] CI with cross-platform tests for multiple compilers

### Reporting Issues

If you find a bug or have a feature request, please [open an issue](https://github.com/walker84837/includium/issues) with:

1. A clear description of the problem
2. Steps to reproduce
3. Expected vs actual behavior
4. Platform and compiler information

## License

This project is licensed under the Mozilla Public License Version 2.0. See the [LICENSE](LICENSE) file for details.
