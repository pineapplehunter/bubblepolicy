# AGENTS.md - Agent Guidelines for This Project

A Rust CLI tool for configuring bubblewrap sandboxes with SELinux-style workflow.

## Project Overview

- `src/main.rs` - CLI entry point with clap
- `src/trace.rs` - Trace subcommand (strace wrapper) with optional output file
- `src/review.rs` - Review subcommand (TUI file tree toggler) with optional output file
- `src/create.rs` - Create subcommand (wrapper generator) with directory optimization
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
- Use `color_eyre` for error handling (Result<T, eyre::Report>)
- Use `log` + `env_logger` for logging

### Error Handling with color_eyre
```rust
use color_eyre::{Result, eyre::{WrapErr, bail}};

// For context on errors:
let x = operation().context("Failed to do thing")?;

// For bailing early with an error:
if condition {
    bail!("Something went wrong: {}", reason);
}
```

### Naming
- `snake_case` for functions/variables
- `CamelCase` for types/enums
- Descriptive names: `allowed_paths` not `paths`
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
Each subcommand is a module:
```rust
pub fn run(args: &[Type]) -> Result<()> {
    // implementation
    Ok(())
}
```

## Development

### Adding Dependencies
```bash
cargo add <package>          # Add latest version
cargo add <package> --vers 1.0  # Add specific version
```

### Adding New Subcommands
1. Add variant to `Commands` enum in `main.rs`
2. Create module in `src/<command>.rs` with `pub fn run() -> Result<()>`
3. Add `pub mod <command>;` to `src/lib.rs`
4. Add handler in `main.rs` match block

## Working with This Repository

1. **Always run `cargo fmt`** before committing
2. **Always run `cargo clippy`** to catch issues
3. **Test with `cargo test`** before submitting
4. Build release with `cargo build --release`
5. Use `color_eyre::eyre::{bail, WrapErr}` for error handling

### Testing

Use the convenience test script to verify functionality:

```bash
# Run all tests
bash test_myjail.sh test

# Run specific tests
bash test_myjail.sh trace
bash test_myjail.sh review
bash test_myjail.sh create
bash test_myjail.sh grouping
bash test_myjail.sh merge

# Build and test everything
bash test_myjail.sh all

# Clean test files
bash test_myjail.sh clean
```

The test script covers:
- Trace command functionality
- Review command with policy generation
- Create command with wrapper generation
- Directory grouping optimization
- Multi-file merge in review

## Notes for Agents

- This is a bubblewrap policy tool with 3 subcommands: `trace`, `review`, `create`
- Uses `ratatui` for TUI (review command), `clap` for CLI parsing, `color_eyre` for errors
- Uses raw `ptrace` via `nix` crate for trace command (not external strace)
- Uses `syscall_numbers` crate for syscall constants
- Output: shell scripts or binary wrappers for bubblewrap
- SELinux-style workflow: audit → review → enforce
- All subcommands support `--output` flag for file output (default: stdout)
- `create` subcommand automatically optimizes directory binds by collapsing paths with identical access
- Policy entries use `Access` enum: `Deny`, `ReadOnly`, `ReadWrite`, `Tmpfs`
- Test data files in `test_data/` directory for manual testing
