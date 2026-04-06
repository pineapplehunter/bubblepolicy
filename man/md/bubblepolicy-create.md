# bubblepolicy-create(1) -- Create a bubblewrap wrapper

## SYNOPSIS

**bubblepolicy create** [_POLICY_] [_BINARY_]

## DESCRIPTION

Create a bubblewrap wrapper script from a policy file. Outputs to stdout.

## OPTIONS

_POLICY_ - Policy file (default: policy.json).

_BINARY_ - Binary to wrap (default: /bin/sh).

## EXAMPLE

    $ bubblepolicy create policy.json /bin/bash > wrapper.sh

## SEE ALSO

bubblepolicy(1)

## AUTHOR

Bubblepolicy authors
