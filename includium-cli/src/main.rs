#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]

use includium::PreprocessorConfig;

fn main() {
    let src = r#"
#ifdef __linux__
#define PLATFORM "Linux"
#else
#define PLATFORM "Other"
#endif
#define PI 3.14
#define ADD(a, b) ((a)+(b))
char* platform = PLATFORM;
float x = PI;
int y = ADD(1, 2);
"#;

    // Test the new high-level API
    let config = PreprocessorConfig::for_linux();
    match includium::process(src, &config) {
        Ok(result) => println!("High-level API result:\n{result}"),
        Err(e) => eprintln!("Error: {e}"),
    }

    // Test with Windows config
    let windows_config = PreprocessorConfig::for_windows();
    match includium::process(src, &windows_config) {
        Ok(result) => println!("\nWindows config result:\n{result}"),
        Err(e) => eprintln!("Error: {e}"),
    }
}
