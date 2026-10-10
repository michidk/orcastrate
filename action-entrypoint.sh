#!/bin/sh
set -eu

config=$1
command=$2
mode=$3
dry_run=$4
set -- --config "$config"
if [ "$dry_run" = true ]; then
    set -- "$@" --dry-run
fi
set -- "$@" "$command"
if [ "$command" = sync ]; then
    set -- "$@" --mode "$mode"
fi

# Empty Action inputs must not select an unconfigured authentication method.
[ -n "${ORCASTRATE_APP_ID:-}" ] || unset ORCASTRATE_APP_ID
[ -n "${ORCASTRATE_PRIVATE_KEY:-}" ] || unset ORCASTRATE_PRIVATE_KEY
[ -n "${ORCASTRATE_INSTALLATION_ID:-}" ] || unset ORCASTRATE_INSTALLATION_ID
[ -n "${ORCASTRATE_TOKEN:-}" ] || unset ORCASTRATE_TOKEN
exec orcastrate "$@"
