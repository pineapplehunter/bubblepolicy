# myjail

A Rust CLI tool for configuring bubblewrap sandboxes using an SELinux-style workflow: trace → review → create.

## AI Disclosure

myjail was developed with AI assistance. This tool helps create sandboxed environments using bubblewrap, and was built using modern AI coding tools to accelerate development while maintaining human oversight and review.

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
  review  Review traced paths and toggle allow/deny in a TUI
  create  Create a bubblewrap wrapper from a policy file
  help    Print this message or the help of the given subcommand(s)
```

### trace

Trace system calls and file access of a command:

```bash
myjail trace -- firefox
```

Save trace to file:

```bash
myjail trace --output trace.json -- firefox
```

### review

Review traced paths in a TUI file tree and toggle allow/deny:

```bash
myjail review trace.json
```

Generate policy without TUI:

```bash
myjail review --generate-policy trace.json > policy.json
```

Save review output to file:

```bash
myjail review --output policy.json trace.json
```

### create

Create a bubblewrap wrapper script from a policy:

```bash
myjail create --policy policy.json --binary /usr/bin/firefox --output firefox-sandbox
```

The wrapper script will automatically:
- Group directories with multiple subdirectories to reduce the number of bind mounts
- Distinguish between read-only and read-write bind mounts
- Include standard system mounts (/proc, /dev, /tmp, /run)

## Workflow

1. **Trace**: Run `myjail trace -- <command>` to trace all system calls and file accesses
2. **Review**: Review the paths in the TUI and toggle which directories should be allowed/denied
3. **Create**: Generate a bubblewrap wrapper script to sandbox your application

Example full workflow:

```bash
# Step 1: Trace
myjail trace --output trace.json -- /usr/bin/firefox

# Step 2: Review (or generate policy directly)
myjail review --generate-policy --output policy.json trace.json

# Step 3: Create wrapper
myjail create --policy policy.json /usr/bin/firefox --output firefox-sandbox

# Step 4: Make executable and run
chmod +x firefox-sandbox
./firefox-sandbox
```

## Output Flags

All subcommands support `--output` flag to save output to a file instead of stdout:

```bash
# Trace to file
myjail trace --output mytrace.json -- command

# Review to file
myjail review --output mypolicy.json trace.json

# Create wrapper to file
myjail create --policy mypolicy.json /bin/program --output wrapper.sh
```

## Requirements

- Linux with bubblewrap installed (`bwrap`)
- `strace` for the trace command

## License

MIT
