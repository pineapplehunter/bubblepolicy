# AGENTS.md - Agent Guidelines for This Project

A Rust CLI tool for configuring bubblewrap sandboxes with SELinux-style workflow: trace → review → optimise → create.

## Project Structure

```
src/
├── main.rs        # CLI entry point with clap
├── lib.rs         # Library exports
├── common.rs      # Shared types: Access, PolicyNode, PolicyTree
├── trace.rs       # Trace subcommand (ptrace-based file access tracing)
├── review.rs      # Review subcommand (CLI tree manipulation)
├── review_ui.rs   # Review subcommand (TUI file tree toggler)
├── optimise.rs    # Optimize subcommand (tree dedup/compression)
└── create.rs      # Create subcommand (bubblewrap wrapper generator)
```

## Data Model

### Tree Format
```json
{
  "entries": [
    {"path": "/", "access": "ReadOnly", "children": [...]}
  ]
}
```

- Only non-deny entries are in the tree (deny is implicit)
- Children inherit parent access unless explicitly overridden
- Access enum: `Deny`, `ReadOnly`, `ReadWrite`, `Tmpfs`

## Build/Lint/Test Commands

### Build
```bash
cargo build              # Debug build
cargo build --release    # Release build (with LTO and strip)
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
2. External crates (`crate::`, `color_eyre::`, etc.)
3. Local modules (`crate::common`)

```rust
use std::fs;
use std::path::Path;

use color_eyre::{eyre::{bail, WrapErr}, Result};

use crate::common::{Access, PolicyNode, PolicyTree};
```

### Error Handling
Use `color_eyre` for all error handling:
```rust
use color_eyre::{Result, eyre::{WrapErr, bail}};

// Context on errors:
fs::read_to_string(file).with_context(|| format!("Failed to read: {}", file))?;

// Early return with error:
if !Path::new(policy).exists() {
    bail!("Policy file not found: {}", policy);
}

// Main function returns Result:
fn main() -> Result<()> {
    color_eyre::install()?;
    // ...
    Ok(())
}
```

### Naming Conventions
- **snake_case**: functions, variables, methods
- **CamelCase**: types, enums, structs
- **SCREAMING_SNAKE_CASE**: constants (rarely used)
- Use descriptive names: `allowed_paths` not `paths`, `is_tmpfs` not `check_tmpfs`

```rust
// Structs and enums
#[derive(Debug, Clone, PartialEq)]
pub enum Access {
    Deny,
    ReadOnly,
    ReadWrite,
    Tmpfs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyNode {
    pub path: String,
    pub access: Access,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PolicyNode>,
}

// Helper methods on enums
impl Access {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Access::Deny)
    }
}
```

### Module Structure
Each subcommand is a separate module with a `run` function:
```rust
pub fn run(file: &str, ro: &[String], rw: &[String], tmp: &[String], deny: &[String]) -> Result<()> {
    // Implementation
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        // Tests go here
    }
}
```

### Testing Patterns
- Unit tests: `#[cfg(test)]` module within source files
- Integration tests: `tests/` directory
- Use helper functions for creating test fixtures
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(path: &str, access: Access) -> PolicyEntry {
        PolicyEntry {
            path: path.to_string(),
            access,
        }
    }

    #[test]
    fn test_something() {
        let entries = vec![make_entry("/test", Access::ReadOnly)];
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

## Testing with test_bubblepolicy.sh

```bash
./test_bubblepolicy.sh all      # Build and run all tests
./test_bubblepolicy.sh test     # Run test suite only
./test_bubblepolicy.sh build    # Build only
./test_bubblepolicy.sh clean    # Clean test files

# Individual tests
./test_bubblepolicy.sh trace    # Test trace command
./test_bubblepolicy.sh review   # Test review command
./test_bubblepolicy.sh create   # Test create command
./test_bubblepolicy.sh grouping  # Test directory grouping
./test_bubblepolicy.sh merge    # Test multi-file merge
```

## Key Implementation Notes

- **ptrace**: Uses raw `ptrace` via `nix` crate (not external strace)
- **syscall constants**: Uses `syscall_numbers` crate
- **Output**: Shell scripts or binary wrappers for bubblewrap
- **Subcommand outputs**: `trace`, `review-ui` use `--output` flag; `create` uses stdout
- **optimise**: Automatically collapses paths with identical access into parent directories

## Dependencies

- `clap`: CLI parsing with derive macros
- `color-eyre`: Error handling with colored reports
- `serde`/`serde_json`: Serialization
- `ratatui`/`crossterm`: TUI for review-ui
- `nix`: Linux syscalls (ptrace)
- `log`/`env_logger`: Logging
- `syscall_numbers`: Syscall constants
