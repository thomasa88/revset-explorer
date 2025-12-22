#!/bin/bash -e

jj git init sample
cd sample

jj commit -m 'First commit'
jj commit -m 'Second commit'
jj new --no-edit -m 'Branch'
jj commit -m 'Third commit'
jj new @ @--

