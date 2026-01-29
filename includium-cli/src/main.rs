#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]

//! # Includium CLI
//!
//! A command-line interface for the includium C preprocessor library.

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use includium::{Compiler, PreprocessorConfig, Target, WarningHandler};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Exit codes for different error conditions
mod exit_code {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const IO_ERROR: i32 = 2;
    pub const PREPROCESS_ERROR: i32 = 3;
    #[allow(dead_code)]
    pub const ARGUMENT_ERROR: i32 = 4;
}

/// Command-line interface for the includium C preprocessor
#[derive(Parser)]
#[command(
    name = "includium",
    version,
    author,
    about = "A C preprocessor implementation in Rust",
    long_about = "includium is a complete C preprocessor implementation that can process C/C++ source code with macros, conditional compilation, and includes.",
    after_help = "EXAMPLES:
  # Preprocess a single file
  $ includium input.c -o output.i

  # Preprocess for Windows with MSVC
  $ includium input.c --target windows --compiler msvc

  # Preprocess with custom include directories
  $ includium input.c -I include -I /usr/include -o output.i

  # Read from stdin and write to stdout
  $ cat input.c | includium - | gcc -x c -

  # Verbose preprocessing with warnings
  $ includium input.c -v --target linux

  # Dry run to see what would happen
  $ includium input.c --dry-run

For more information, visit: https://github.com/walker84837/includium"
)]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Input file to preprocess (use '-' for stdin)
    #[arg(help = "Input C/C++ file to preprocess (use '-' for stdin)")]
    input: PathBuf,

    /// Output file (use '-' for stdout, default: stdout)
    #[arg(
        short = 'o',
        long,
        help = "Output file (use '-' for stdout, default: stdout)"
    )]
    output: Option<PathBuf>,

    /// Target operating system
    #[arg(
        short = 't',
        long,
        value_enum,
        default_value = "linux",
        help = "Target operating system"
    )]
    target: TargetValue,

    /// Compiler dialect
    #[arg(
        short = 'c',
        long,
        value_enum,
        default_value = "gcc",
        help = "Compiler dialect for predefined macros"
    )]
    compiler: CompilerValue,

    /// Add include directory
    #[arg(
        short = 'I',
        long = "include",
        value_name = "DIR",
        help = "Add directory to include search path"
    )]
    include_dirs: Vec<PathBuf>,

    /// Maximum recursion depth for macro expansion
    #[arg(
        long,
        default_value = "128",
        help = "Maximum recursion depth for macro expansion"
    )]
    recursion_limit: usize,

    /// Output in JSON format
    #[arg(long, help = "Output preprocessing result in JSON format")]
    #[cfg(feature = "json")]
    json: bool,

    /// Output in plain text format (no formatting)
    #[arg(long, help = "Output in plain text format for scripts")]
    plain: bool,

    /// Enable verbose output
    #[arg(
        short = 'v',
        long,
        help = "Enable verbose output with diagnostic information"
    )]
    verbose: bool,

    /// Suppress non-error output
    #[arg(short = 'q', long, help = "Suppress non-error output (quiet mode)")]
    quiet: bool,

    /// Show preprocessing warnings
    #[arg(short = 'W', long, help = "Enable preprocessing warnings")]
    warnings: bool,

    /// Show what would happen without preprocessing
    #[arg(
        short = 'n',
        long,
        help = "Show what would happen without actually preprocessing"
    )]
    dry_run: bool,

    /// Disable colored output
    #[arg(long, help = "Disable colored output")]
    no_color: bool,

    /// Force colored output
    #[arg(long, help = "Force colored output even when not a terminal")]
    force_color: bool,
}

/// Target operating system values for CLI
#[derive(Clone, Debug, ValueEnum)]
enum TargetValue {
    Linux,
    Windows,
    #[clap(name = "mac-os")]
    MacOS,
}

impl From<TargetValue> for Target {
    fn from(value: TargetValue) -> Self {
        match value {
            TargetValue::Linux => Target::Linux,
            TargetValue::Windows => Target::Windows,
            TargetValue::MacOS => Target::MacOS,
        }
    }
}

/// Compiler dialect values for CLI
#[derive(Clone, Debug, ValueEnum)]
#[allow(clippy::upper_case_acronyms)]
enum CompilerValue {
    #[clap(name = "gcc")]
    GCC,
    Clang,
    #[clap(name = "msvc")]
    MSVC,
}

impl From<CompilerValue> for Compiler {
    fn from(value: CompilerValue) -> Self {
        match value {
            CompilerValue::GCC => Compiler::GCC,
            CompilerValue::Clang => Compiler::Clang,
            CompilerValue::MSVC => Compiler::MSVC,
        }
    }
}

/// Global flag to track if any warnings occurred
static WARNINGS_OCCURRED: AtomicBool = AtomicBool::new(false);

/// Main application entry point
fn main() {
    std::process::exit(match run() {
        Ok(_) => {
            if WARNINGS_OCCURRED.load(Ordering::Relaxed) {
                exit_code::GENERAL_ERROR
            } else {
                exit_code::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            determine_exit_code(&e)
        }
    });
}

/// Determine the appropriate exit code based on the error
fn determine_exit_code(error: &anyhow::Error) -> i32 {
    if let Some(io_err) = error.downcast_ref::<std::io::Error>() {
        match io_err.kind() {
            std::io::ErrorKind::NotFound => exit_code::IO_ERROR,
            std::io::ErrorKind::PermissionDenied => exit_code::IO_ERROR,
            _ => exit_code::IO_ERROR,
        }
    } else if error.downcast_ref::<includium::PreprocessError>().is_some() {
        exit_code::PREPROCESS_ERROR
    } else {
        exit_code::GENERAL_ERROR
    }
}

/// Run the main application logic
fn run() -> Result<()> {
    let cli = Cli::parse();

    // Validate arguments
    validate_args(&cli)?;

    // Show dry run information and exit
    if cli.dry_run {
        show_dry_run_info(&cli);
        return Ok(());
    }

    // Read input
    let input_content = read_input(&cli.input)?;

    // Create preprocessor configuration
    let config = create_config(&cli)?;

    // Preprocess the input
    let start_time = std::time::Instant::now();
    let processed_output = match includium::process(&input_content, &config) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Preprocessing error: {:#?}", e);
            return Err(anyhow::anyhow!("Failed to preprocess input: {}", e));
        }
    };
    let processing_time = start_time.elapsed();

    // Write output
    write_output(&cli, &processed_output)?;

    // Show verbose information
    if cli.verbose {
        show_verbose_info(&cli, processing_time);
    }

    // Show success message in verbose mode
    if cli.verbose && !cli.quiet {
        let input_display = format_input(&cli.input);
        let output_display = cli
            .output
            .as_ref()
            .map_or("stdout".to_string(), format_output);
        eprintln!("✓ Preprocessed {input_display} -> {output_display}");
    }

    Ok(())
}

/// Validate command-line arguments
fn validate_args(cli: &Cli) -> Result<()> {
    // Check that input and output are not the same file
    if let Some(output) = &cli.output
        && output != &PathBuf::from("-")
        && std::fs::canonicalize(output).ok() == std::fs::canonicalize(&cli.input).ok()
    {
        return Err(anyhow::anyhow!(
            "Input and output files cannot be the same: {}",
            output.display()
        ));
    }

    // Validate recursion limit
    if cli.recursion_limit == 0 {
        return Err(anyhow::anyhow!("Recursion limit must be greater than 0"));
    }

    Ok(())
}

/// Show dry run information
fn show_dry_run_info(cli: &Cli) {
    let input_display = format_input(&cli.input);
    let output_display = cli
        .output
        .as_ref()
        .map_or("stdout".to_string(), format_output);

    eprintln!("Dry run: would preprocess {input_display} -> {output_display}");
    eprintln!("Target: {}", format_target(&cli.target));
    eprintln!("Compiler: {}", format_compiler(&cli.compiler));
    eprintln!("Recursion limit: {}", cli.recursion_limit);

    if !cli.include_dirs.is_empty() {
        eprintln!("Include directories:");
        for dir in &cli.include_dirs {
            eprintln!("  {}", dir.display());
        }
    }

    #[cfg(feature = "json")]
    if cli.json {
        eprintln!("Output format: JSON");
    } else if cli.plain {
        eprintln!("Output format: Plain text");
    }
}

/// Create preprocessor configuration from CLI arguments
fn create_config(cli: &Cli) -> Result<PreprocessorConfig> {
    let target: Target = cli.target.clone().into();
    let compiler: Compiler = cli.compiler.clone().into();

    let mut config = match target {
        Target::Linux => PreprocessorConfig::for_linux().with_compiler(compiler),
        Target::Windows => PreprocessorConfig::for_windows().with_compiler(compiler),
        Target::MacOS => PreprocessorConfig::for_macos().with_compiler(compiler),
    };

    // Set recursion limit
    config.recursion_limit = cli.recursion_limit;

    // Setup warning handler if warnings are enabled
    if cli.warnings {
        let warning_handler = create_warning_handler(cli);
        config.warning_handler = Some(warning_handler);
    }

    Ok(config)
}

/// Create a warning handler
fn create_warning_handler(cli: &Cli) -> WarningHandler {
    let show_warnings = cli.warnings;
    let quiet = cli.quiet;

    Rc::new(move |message: &str| {
        if show_warnings && !quiet {
            WARNINGS_OCCURRED.store(true, Ordering::Relaxed);
            eprintln!("Warning: {}", message);
        }
    })
}

/// Read input from file or stdin
fn read_input(input_path: &PathBuf) -> Result<String> {
    if input_path == &PathBuf::from("-") {
        // Read from stdin
        use std::io::Read;
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        Ok(buffer)
    } else {
        // Read from file
        std::fs::read_to_string(input_path)
            .with_context(|| format!("Failed to read input file: {}", input_path.display()))
    }
}

/// Write output to file or stdout
fn write_output(cli: &Cli, content: &str) -> Result<()> {
    #[cfg(feature = "json")]
    if cli.json {
        return write_json_output(cli, content);
    }

    let output_content = content.to_string();

    match &cli.output {
        Some(output_path) if output_path != &PathBuf::from("-") => {
            std::fs::write(output_path, output_content).with_context(|| {
                format!("Failed to write to output file: {}", output_path.display())
            })?;
        }
        _ => {
            // Write to stdout
            print!("{}", output_content);
        }
    }

    Ok(())
}

/// Write JSON output
#[cfg(feature = "json")]
fn write_json_output(cli: &Cli, content: &str) -> Result<()> {
    use serde_json::json;

    let result = json!({
        "success": true,
        "output": content,
        "input_file": format_input(&cli.input),
        "output_file": cli.output.as_ref().map(format_output),
        "target": format_target(&cli.target),
        "compiler": format_compiler(&cli.compiler),
        "include_dirs": cli.include_dirs.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "processing_time_ms": 0 // Would need to measure this
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// Show verbose information
fn show_verbose_info(cli: &Cli, processing_time: std::time::Duration) {
    if cli.quiet {
        return;
    }

    eprintln!("Target: {}", format_target(&cli.target));
    eprintln!("Compiler: {}", format_compiler(&cli.compiler));
    eprintln!("Recursion limit: {}", cli.recursion_limit);
    eprintln!("Processing time: {:?}", processing_time);

    if !cli.include_dirs.is_empty() {
        eprintln!("Include directories ({}):", cli.include_dirs.len());
        for dir in &cli.include_dirs {
            eprintln!("  {}", dir.display());
        }
    }
}

/// Format input path for display
fn format_input(path: &PathBuf) -> String {
    if path == &PathBuf::from("-") {
        "stdin".to_string()
    } else {
        path.display().to_string()
    }
}

/// Format output path for display
fn format_output(path: &PathBuf) -> String {
    if path == &PathBuf::from("-") {
        "stdout".to_string()
    } else {
        path.display().to_string()
    }
}

/// Format target for display
fn format_target(target: &TargetValue) -> String {
    match target {
        TargetValue::Linux => "Linux".to_string(),
        TargetValue::Windows => "Windows".to_string(),
        TargetValue::MacOS => "macOS".to_string(),
    }
}

/// Format compiler for display
fn format_compiler(compiler: &CompilerValue) -> String {
    match compiler {
        CompilerValue::GCC => "GCC".to_string(),
        CompilerValue::Clang => "Clang".to_string(),
        CompilerValue::MSVC => "MSVC".to_string(),
    }
}
