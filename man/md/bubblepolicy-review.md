# bubblepolicy-review(1) -- Manipulate tree attributes via CLI

## SYNOPSIS

**bubblepolicy review** _FILE_ [_OPTIONS_]

## DESCRIPTION

Manipulate tree attributes via command-line interface.

## OPTIONS

_FILE_ - Input/output file (required).

**-r**, **--ro** _PATH_ - Set paths as read-only.

**-w**, **--rw** _PATH_ - Set paths as read-write.

**-t**, **--tmp** _PATH_ - Set paths as tmpfs.

**-d**, **--deny** _PATH_ - Set paths as deny.

## EXAMPLE

    $ bubblepolicy review policy.txt --ro /etc/passwd --rw /tmp

## SEE ALSO

bubblepolicy(1), bubblepolicy-review-ui(1)

## AUTHOR

Bubblepolicy authors
