#!/bin/sh
set -eu
test_dir=$(mktemp -d)
trap 'rm -r "$test_dir"' EXIT
ln -s /bin/echo "$test_dir/orcastrate"
export PATH="$test_dir:$PATH"
actual=$(sh action-entrypoint.sh orchestrator.toml validate pr false)
[ "$actual" = '--config orchestrator.toml validate' ]
actual=$(sh action-entrypoint.sh 'config with spaces.toml' sync issue true)
[ "$actual" = '--config config with spaces.toml --dry-run sync --mode issue' ]
actual=$(sh action-entrypoint.sh orchestrator.toml drift pr false)
[ "$actual" = '--config orchestrator.toml drift' ]
