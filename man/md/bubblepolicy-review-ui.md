# bubblepolicy-review-ui(1) -- Review traced paths in TUI

## SYNOPSIS

**bubblepolicy review-ui** _FILE_

## DESCRIPTION

Review traced paths in a terminal user interface and toggle allow/deny permissions.

## OPTIONS

_FILE_ - Input/output file (required). The file must contain traced paths in the policy format.

## KEYBOARD CONTROLS

- **Arrow keys** or **hjkl**: Navigate up/down/left/right in the tree
- **Space**: Expand or collapse directories
- **d**: Set to Deny (✗)
- **r**: Set to ReadOnly (◐)
- **w**: Set to ReadWrite (●)
- **t**: Set to Tmpfs (◆)
- **p**: Set to Partial/Mixed (○)
- **D**: Toggle debug view
- **q**: Quit and save

## EXAMPLE

    $ bubblepolicy review-ui policy.txt

## SEE ALSO

bubblepolicy(1), bubblepolicy-review(1)

## AUTHOR

Bubblepolicy authors
