# AGENTS.md - Agent Guidelines for This Project

A Rust CLI tool for configuring bubblewrap sandboxes with SELinux-style workflow.

## Project Overview

- `src/main.rs` - CLI entry point with clap
- `src/` - Module-based architecture (trace, scan, create)
- `Cargo.toml` - Rust dependencies

## Build/Lint/Test Commands

### Build
```bash
cargo build          # Debug build
cargo build --release  # Release build
cargo run -- [args]  # Run with args
```

### Format & Lint
```bash
cargo fmt            # Format code
cargo fmt -- --check  # Check formatting
cargo clippy         # Lint with clippy
cargo clippy -- -D warnings  # Strict lint
```

### Test
```bash
cargo test           # Run all tests
cargo test <name>    # Run specific test
cargo test -- --nocapture  # Show output
```

### Check
```bash
cargo check          # Type check only
cargo doc            # Generate docs
```

## Code Style Guidelines

### General
- Use 4-space indentation for Rust
- Follow `rustfmt` default style
- Use `anyhow` for error handling
- Use `log` + `env_logger` for logging

### Naming
- `snake_case` for functions/variables
- `CamelCase` for types/enums
- Descriptive names: `allowed_paths` not `paths`

### Error Handling
- Use `anyhow::Result<T>` for functions that can fail
- Use `anyhow::bail!("message")` for early returns
- Provide context in errors: `context("failed to open file")`

### Structs & Enums
```rust
#[derive(Clone, Debug)]
struct Config {
    allow: Vec<PathBuf>,
    deny: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
enum Permission {
    Allow,
    Deny,
}
```

### Module Structure
```rust
mod trace;
mod scan;
mod create;

pub fn run() -> Result<()> { ... }
```

## Development

### Adding Dependencies
1. Add to `Cargo.toml` under `[dependencies]`
2. Run `cargo check` to fetch and verify
3. Rebuild with `cargo build`

### Adding Commands
1. Add variant to `Commands` enum in `main.rs`
2. Create module in `src/<command>.rs`
3. Implement `pub fn run() -> Result<()>`

## Working with This Repository

1. **Always run `cargo fmt`** before committing
2. **Always run `cargo clippy`** to catch issues
3. **Test with `cargo test`** before submitting
4. Build release with `cargo build --release`

## Notes for Agents

- This is a bubblewrap policy tool with 3 subcommands: `trace`, `scan`, `create`
- Uses `ratatui` for TUI, `clap` for CLI parsing
- Requires `strace` system package for trace command
- Output: shell scripts or binary wrappers for bubblewrap
