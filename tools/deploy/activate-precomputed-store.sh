#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s <candidate-store-dir> <active-store-symlink>\n' "$0" >&2
}

if [ "$#" -ne 2 ]; then
    usage
    exit 2
fi

candidate_dir=$1
active_link=$2

if [ ! -d "$candidate_dir" ]; then
    printf 'candidate store is not a directory: %s\n' "$candidate_dir" >&2
    exit 1
fi

manifest_count=$(find "$candidate_dir" -mindepth 2 -maxdepth 2 -name manifest.json -type f | wc -l | tr -d ' ')
if [ "$manifest_count" = "0" ]; then
    printf 'candidate store has no seed manifest: %s\n' "$candidate_dir" >&2
    exit 1
fi

active_parent=$(dirname "$active_link")
mkdir -p "$active_parent"

tmp_link="${active_link}.next"
ln -sfn "$(realpath "$candidate_dir")" "$tmp_link"
mv -Tf "$tmp_link" "$active_link"

printf 'activated precomputed store: %s -> %s\n' "$active_link" "$(realpath "$candidate_dir")"
