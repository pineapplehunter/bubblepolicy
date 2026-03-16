# myjail

A Rust CLI tool for configuring bubblewrap sandboxes using an SELinux-style workflow: trace → review → create.

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
```

## Usage

```
myjail - Bubblewrap sandbox policy tool

Usage: myjail <COMMAND>

Commands:
  trace   Trace system calls and file access of a command
  scan    Review traced paths and toggle allow/deny in a TUI
  create  Create a bubblewrap wrapper from a policy file
  help    Print this message or the help of the given subcommand(s)
```

### trace

Trace system calls and file access of a command:

```bash
myjail trace -- firefox
```

### scan

Review traced paths in a TUI file tree and toggle allow/deny:

```bash
myjail scan /path/to/trace/output.json
```

### create

Create a bubblewrap wrapper script or binary from a policy:

```bash
myjail create --policy policy.json --binary /usr/bin/firefox --output firefox-sandbox
```

## Workflow

1. **Trace**: Run `myjail trace -- <command>` to trace all system calls and file accesses
2. **Scan**: Review the paths in the TUI and toggle which directories should be allowed/denied
3. **Create**: Generate a bubblewrap wrapper script to sandbox your application

## Requirements

- Linux with bubblewrap installed (`bwrap`)
- `strace` for the trace command

## License

MIT
