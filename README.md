# bubblepolicy

A Rust CLI tool for configuring bubblewrap sandboxes using an SELinux-style workflow: trace → review → optimise → create.

## AI Disclosure

bubblepolicy was developed with AI assistance. This tool helps create sandboxed environments using bubblewrap, and was built using modern AI coding tools to accelerate development while maintaining human oversight and review.

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
bubblepolicy - Bubblewrap sandbox policy tool

Usage: bubblepolicy <COMMAND>

Commands:
  trace     Trace system calls and file access of a command
  review    Review and manipulate tree attributes via CLI
  review-ui Review traced paths in TUI and toggle allow/deny
  optimise  Optimise/dedup the policy tree
  create    Create a bubblewrap wrapper from a policy file
  help      Print this message or the help of the given subcommand(s)
```

### trace

Trace system calls and file access of a command:

```bash
bubblepolicy trace trace.json -- firefox
```

Output format (tree with default values):

```json
{
  "entries": [
    {"path": "/", "access": "ReadOnly", "children": [...]}
  ]
}
```

### review-ui

Review traced paths in a TUI file tree and toggle allow/deny:

```bash
bubblepolicy review-ui trace.json
```

This will update the contents of `trace.json`.

### review

Manipulate tree attributes via CLI (for scripting):

```bash
# Set path access
bubblepolicy review trace.json --ro /nix --rw /home --tmp /run
```

This updates the tree attributes inplace.

### optimise

Optimise/dedup the policy tree inplace (collapse same-access siblings):

```bash
bubblepolicy optimise trace.json
```

This reduces redundant entries by collapsing directories with identical access.
The format of this is a list of trees.

### create

Create a bubblewrap wrapper script from a policy:

```bash
bubblepolicy create optimised.json
```

The wrapper script will automatically:
- Use unshare-all to create a sandbox.
- Distinguish between read-only and read-write bind mounts
- Include standard system mounts (/proc, /dev, /tmp, /run)

## Workflow

1. **Trace**: Run `bubblepolicy trace --output trace.json -- <command>` to trace all system calls and file accesses
2. **Review**: Review the paths in TUI (`review-ui`) or CLI (`review`) to toggle which directories should be allowed/denied
3. **Optimise**: Dedup the policy tree to reduce redundant entries
4. **Create**: Generate a bubblewrap wrapper script to sandbox your application

Example full workflow:

```bash
# Step 1: Trace
bubblepolicy trace trace.json -- /usr/bin/firefox

# Step 2: Review (TUI)
bubblepolicy review-ui trace.json

# Step 3: Review (CLI alternative)
bubblepolicy review trace.json -r /etc -w /home

# Step 4: Optimise
bubblepolicy optimise trace.json

# Step 5: Create wrapper
bubblepolicy create trace.json /usr/bin/firefox > firefox-sandbox

# Step 6: Make executable and run
chmod +x firefox-sandbox
./firefox-sandbox
```

## Requirements

- Linux with bubblewrap installed (`bwrap`)

## License

MIT
