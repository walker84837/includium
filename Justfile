#!/usr/bin/env just --list

# Default recipe
default: check

# Build the project
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run tests
test:
    cargo test --workspace

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# Check code formatting
check-fmt:
    cargo fmt --all --check

# Format code
fmt:
    cargo fmt --all

# Run clippy lints
clippy:
    cargo clippy --workspace -- -D warnings

# Run all checks (fmt, clippy, test)
check: check-fmt clippy test

# Run the CLI
run +args:
    cargo run -p includium-cli -- {{args}}

# Run the CLI in release mode
run-release +args:
    cargo run -p includium-cli --release -- {{args}}

# Clean build artifacts
clean:
    cargo clean

# Generate documentation
docs:
    cargo doc --workspace

# Watch for changes and run tests
watch:
    cargo watch -x test

# Install the CLI
install:
    cargo install --path includium-cli --locked

# Create a release build and package on Linux
package: build-release
    tar czf includium-cli-$(git describe --tags --always)-$(uname -s)-$(uname -m).tar.gz -C target/release includium-cli
