# bubblepolicy(1) -- Bubblewrap sandbox policy tool

## SYNOPSIS

**bubblepolicy** [_COMMAND_] [_OPTIONS_]

## DESCRIPTION

Bubblepolicy is a tool for configuring bubblewrap sandboxes with a SELinux-style workflow: trace, review, optimise, create.

## COMMANDS

- **trace** - Trace system calls and file access of a command
- **review-ui** - Review traced paths in TUI and toggle allow/deny
- **review** - Manipulate tree attributes via CLI
- **optimise** - Optimise/dedup the policy tree (in place)
- **create** - Create a bubblewrap wrapper from a policy file

## SEE ALSO

bubblepolicy-trace(1), bubblepolicy-review-ui(1), bubblepolicy-review(1), bubblepolicy-optimise(1), bubblepolicy-create(1)

## AUTHOR

Bubblepolicy authors
