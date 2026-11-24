#!/usr/bin/env bash
# Copyright (c) 2026-present, the Zendia developers.
#
# SPDX-License-Identifier: BSD-2-Clause

# Lists the upstream files that Zendia changes, meaning every file outside Zendia/ that differs from upstream Ladybird,
# and fails if one of them is missing from Zendia/FORK.md. It also names the entries in FORK.md that no longer differ,
# so they can be removed.
#
# Usage: Zendia/Meta/fork-surface.sh [upstream-ref]
# The upstream ref defaults to upstream/master if there is an "upstream" remote, and to origin/master otherwise.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

upstream_ref="${1:-}"
if [[ -z "$upstream_ref" ]]; then
    if git rev-parse --quiet --verify upstream/master >/dev/null; then
        upstream_ref="upstream/master"
    else
        upstream_ref="origin/master"
    fi
fi

base="$(git merge-base HEAD "$upstream_ref")"
ledger="Zendia/FORK.md"

mapfile -t changed < <(git diff --name-only "$base" HEAD -- . ':(exclude)Zendia' ':(exclude,glob).github/workflows/zendia-*.yml')

missing=()
for file in "${changed[@]}"; do
    if ! grep --quiet --fixed-strings -- "- \`$file\`" "$ledger"; then
        missing+=("$file")
    fi
done

stale=()
while IFS= read -r file; do
    if ! printf '%s\n' "${changed[@]}" | grep --quiet --line-regexp --fixed-strings -- "$file"; then
        stale+=("$file")
    fi
done < <(sed -n 's/^- `\([^`]*\)`$/\1/p' "$ledger")

echo "Upstream files changed: ${#changed[@]} (against $upstream_ref at $(git rev-parse --short "$base"))"

if (( ${#stale[@]} )); then
    echo "Listed in $ledger but no longer changed, so they can be removed from it:"
    printf '  %s\n' "${stale[@]}"
fi

if (( ${#missing[@]} )); then
    echo "Changed but not listed in $ledger. Add each one with the reason for the change:"
    printf '  %s\n' "${missing[@]}"
    exit 1
fi
