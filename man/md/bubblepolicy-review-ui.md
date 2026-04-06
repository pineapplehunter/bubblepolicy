# bubblepolicy-review-ui(1) -- Review traced paths in TUI

## SYNOPSIS

**bubblepolicy review-ui** _FILE_

## DESCRIPTION

Review traced paths in a terminal user interface and toggle allow/deny permissions.

## OPTIONS

_FILE_ - Input/output file (required). The file must contain traced paths in the policy format.

## INTERACTION

Use arrow keys to navigate the file tree. Press Space to toggle access mode (ReadOnly, ReadWrite, Tmpfs, Deny).

## EXAMPLE

    $ bubblepolicy review-ui policy.txt

## SEE ALSO

bubblepolicy(1), bubblepolicy-review(1)

## AUTHOR

Bubblepolicy authors
