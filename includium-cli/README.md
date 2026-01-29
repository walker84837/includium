# includium-cli

A command-line interface for the includium C preprocessor library.

## Installation

### From source

```bash
git clone https://github.com/walker84837/includium.git
cd includium
cargo install --path includium-cli
```

## Usage

Basic usage:

```bash
# Preprocess a single file
includium input.c -o output.i

# Preprocess for Windows with MSVC
includium input.c --target windows --compiler msvc

# Preprocess with custom include directories
includium input.c -I include -I /usr/include -o output.i

# Read from stdin and write to stdout
cat input.c | includium - | gcc -x c -

# Verbose preprocessing with warnings
includium input.c -v --target linux

# Dry run to see what would happen
includium input.c --dry-run
```

## Options

### Input/Output

- `<INPUT>`: Input C/C++ file to preprocess (use '-' for stdin)
- `-o, --output <OUTPUT>`: Output file (use '-' for stdout, default: stdout)

### Target Configuration

- `-t, --target <TARGET>`: Target operating system [default: linux]  
  Possible values: linux, windows, mac-os
- `-c, --compiler <COMPILER>`: Compiler dialect [default: gcc]
  Possible values: gcc, clang, msvc
- `-I, --include <DIR>`: Add directory to include search path
- `--recursion-limit <LIMIT>`: Maximum recursion depth for macro expansion [default: 128]

### Output Formatting

- `--json`: Output preprocessing result in JSON format
- `--plain`: Output in plain text format for scripts

### Verbosity and Control

- `-v, --verbose`: Enable verbose output with diagnostic information
- `-q, --quiet`: Suppress non-error output (quiet mode)
- `-W, --warnings`: Enable preprocessing warnings
- `-n, --dry-run`: Show what would happen without actually preprocessing
- `--no-color`: Disable colored output
- `--force-color`: Force colored output even when not a terminal

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: File I/O error
- `3`: Preprocessing error
- `4`: Invalid arguments

## Examples

### Basic preprocessing

```bash
$ includium example.c -o processed.c
```

### Cross-compilation

```bash
# Compile for Windows
includium source.c --target windows --compiler msvc -o win_processed.c

# Compile for macOS
includium source.c --target mac-os --compiler clang -o mac_processed.c
```

### Include directories

```bash
includium source.c -I ./include -I /usr/local/include -o out.c
```

### JSON output

```bash
$ includium source.c --json
{
  "success": true,
  "output": "/* preprocessed code */",
  "input_file": "source.c",
  "output_file": null,
  "target": "Linux",
  "compiler": "GCC",
  "include_dirs": [],
  "processing_time_ms": 0
}
```

### Verbose output with warnings

```bash
$ includium source.c -v --warnings
Target: Linux
Compiler: GCC
Recursion limit: 128
Processing time: 125.75µs
Warning: #warning: This is a warning message
✓ Preprocessed source.c -> stdout
```

## Features

- **Complete preprocessor**: Supports all C preprocessor directives
- **Target-specific**: Predefined macros for Linux, Windows, macOS
- **Compiler dialects**: Support for GCC, Clang, MSVC
- **Include resolution**: Custom include directory support
- **Machine-readable output**: JSON format for integration
- **Error handling**: Detailed error messages with location information
- **Composable**: Can be used in pipelines with other tools

## Integration

The includium CLI is designed to work well with other development tools:

### With GCC/Clang

```bash
includium source.c - | gcc -x c - -o program
includium source.c -o - | clang -x c - -o program
```

### With Make

```makefile
%.o: %.c
	includium $< -o $*.i
	gcc -c $*.i -o $@
```

### With CMake

```cmake
set(CMAKE_C_PREPROCESSOR includium)
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -Xpreprocessor -P")
```

## License

Licensed under the Mozilla Public License 2.0.