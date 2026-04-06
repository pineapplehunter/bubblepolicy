# bubblepolicy-trace(1) -- Trace system calls and file access

## SYNOPSIS

**bubblepolicy trace** [_OUTPUT_] -- _COMMAND_ [_ARGS_...]

## DESCRIPTION

Trace system calls and file access of a command using strace.

## OPTIONS

_OUTPUT_ - Output file (default: stdout). Use '-' for stdout.

**--** - Separator before command to trace.

_COMMAND_ - Command to trace (with arguments).

## EXAMPLE

    $ bubblepolicy trace output.txt -- /bin/ls /tmp

## SEE ALSO

bubblepolicy(1)

## AUTHOR

Bubblepolicy authors
