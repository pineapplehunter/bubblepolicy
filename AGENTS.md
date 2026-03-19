# AGENTS.md - Agent Guidelines for This Project

A Rust CLI tool for configuring bubblewrap sandboxes with SELinux-style workflow: trace → review → optimise → create.

## Project Structure

```
src/
├── main.rs        # CLI entry point with clap
├── lib.rs         # Library exports
├── common.rs      # Shared types: Access, PolicyNode, PolicyTree
├── trace.rs       # Trace subcommand (uses external strace)
├── review.rs      # Review subcommand (CLI tree manipulation)
├── review_ui.rs   # Review subcommand (TUI file tree toggler)
├── optimise.rs    # Optimize subcommand (tree dedup/compression)
├── create.rs      # Create subcommand (bubblewrap wrapper generator)
└── template.sh    # Shell script template for create output
```

## Data Model

```json
[
  {
    "entries": [
      {"path": "/", "access": "ReadOnly", "children": [...]}
    ]
  }
]
```

- Output is a list of trees (absolute paths + relative paths)
- Only non-deny entries are in the tree (deny is implicit)
- Children inherit parent access unless explicitly overridden
- Access enum: `Deny`, `ReadOnly`, `ReadWrite`, `Tmpfs`

## Build/Lint/Test Commands

### Build
```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run -- [args]      # Run with args
```

### Format & Lint
```bash
cargo fmt                # Format code
cargo fmt -- --check     # Check formatting without changes
cargo clippy             # Lint with clippy
cargo clippy -- -D warnings  # Strict lint (fail on warnings)
```

### Test
```bash
cargo test               # Run all tests
cargo test <name>        # Run specific test (e.g., cargo test test_set_node_access)
cargo test -- --nocapture  # Show stdout/stderr output
cargo test --lib         # Run only library tests
cargo test --test '*'    # Run only integration tests
```

### Other
```bash
cargo check              # Type check only (fast)
cargo doc                # Generate documentation
cargo clean              # Clean build artifacts
```

## Code Style Guidelines

### Imports Organization
Group imports in order (rustfmt handles this automatically):
1. Standard library (`std::`)
2. External crates (`color_eyre::`, `ratatui::`, etc.)
3. Local modules (`crate::common`)

```rust
use color_eyre::{Result, eyre::{WrapErr, bail}};
use std::fs;
use std::path::Path;

use crate::common::{Access, PolicyNode, PolicyTree};
```

### Error Handling
Use `color_eyre` for all error handling:
```rust
use color_eyre::{Result, eyre::{WrapErr, bail}};

fs::read_to_string(file).with_context(|| format!("Failed to read: {}", file))?;

if !Path::new(policy).exists() {
    bail!("Policy file not found: {}", policy);
}

fn main() -> Result<()> {
    color_eyre::install()?;
    Ok(())
}
```

### Naming Conventions
- **snake_case**: functions, variables, methods
- **CamelCase**: types, enums, structs
- Use descriptive names: `is_tmpfs` not `check_tmpfs`, `is_allowed` not `check`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Access {
    Deny,
    ReadOnly,
    ReadWrite,
    Tmpfs,
}

impl Access {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Access::Deny)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyNode {
    pub path: String,
    pub access: Access,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PolicyNode>,
}
```

### Module Structure
Each subcommand is a separate module with a `run` function returning `Result<()>`.

### Testing Patterns
- Unit tests: `#[cfg(test)]` module within source files
- Integration tests: `tests/` directory
- Helper functions for creating test fixtures

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(path: &str, access: Access) -> PolicyEntry {
        PolicyEntry { path: path.to_string(), access }
    }

    #[test]
    fn test_something() {
        // assertions
    }
}
```

## Development Workflow

### Adding New Subcommands
1. Add variant to `Commands` enum in `main.rs`
2. Create module in `src/<command>.rs` with `pub fn run() -> Result<()>`
3. Add `pub mod <command>;` to `src/lib.rs`
4. Add handler in `main.rs` match block

### Adding Dependencies
```bash
cargo add <package>           # Add latest version
cargo add <package> --vers 1.0 # Add specific version
```

## Key Implementation Notes

- **trace**: Uses external `strace` command (not raw ptrace)
- **Output**: `trace`, `review-ui` use `--output` flag; `create` outputs to stdout
- **optimise**: Collapses paths with identical access into parent directories
- **create**: Generates shell script using `template.sh` with `include_str!()`

## Dependencies

- `clap`: CLI parsing with derive macros
- `color-eyre`: Error handling with colored reports
- `serde`/`serde_json`: Serialization
- `ratatui`: TUI for review-ui
- `crossterm`: Terminal support (via ratatui)
