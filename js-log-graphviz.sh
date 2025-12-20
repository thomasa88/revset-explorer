#!/bin/bash

FILTER_REVSET="$1"
ALL_REVSET="present(@) | ancestors(immutable_heads().., 2) | present(trunk())"

(
  echo "digraph {"
  jj log -G -r "$ALL_REVSET" -T 'parents.map(|p| change_id.shortest() ++ " -> " ++ p.change_id().shortest()) ++ "\n"'
  jj log -G -r "$FILTER_REVSET" -T 'change_id.shortest() ++ " [style=filled]\n"'
  echo "}"
) | dot -Tpng -o template.png
