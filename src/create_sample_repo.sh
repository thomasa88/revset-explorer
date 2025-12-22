#!/bin/bash

# Explicitly enable errors using `set` as the shebang will be ignored when the
# script is called from revset-explorer.
set -e

jj git init revset-sample
cd revset-sample

jj commit -m 'First commit'
jj commit -m 'Second commit'
jj commit -m 'Third commit'
jj new @-- -m 'Branch'
jj new -m 'First branch commit'
jj new 'heads(::)' -m 'Merge'
jj new --no-edit -m 'Another head'
jj new -m 'Commit'
jj new -m 'Head'
jj edit @-
