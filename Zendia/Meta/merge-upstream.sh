#!/usr/bin/env bash
# Copyright (c) 2026-present, the Zendia developers.
#
# SPDX-License-Identifier: BSD-2-Clause

# Merges upstream Ladybird into the current branch, then checks the fork's changes to upstream files against
# Zendia/FORK.md. Run it weekly: small merges are easy merges. We merge rather than rebase, because other people build
# on the fork's history.
#
# Usage: Zendia/Meta/merge-upstream.sh [remote]
# The remote defaults to "upstream" if it exists, and to "origin" otherwise. To add it:
#   git remote add upstream https://github.com/LadybirdBrowser/ladybird.git

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

remote="${1:-}"
if [[ -z "$remote" ]]; then
    if git remote get-url upstream >/dev/null 2>&1; then
        remote="upstream"
    else
        remote="origin"
    fi
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "There are uncommitted changes. Commit or stash them before merging upstream."
    exit 1
fi

git fetch "$remote"

# NB: Upstream's Meta/lint-commit.sh wants a "Category: Title" commit title, which git's default merge message lacks.
title="Zendia: Merge upstream Ladybird at $(git rev-parse --short "$remote/master")"
if ! git merge --no-ff -m "$title" "$remote/master"; then
    echo "The merge stopped on conflicts. They can only be in the files Zendia/FORK.md lists. Resolve them, commit, then"
    echo "run Zendia/Meta/fork-surface.sh $remote/master."
    exit 1
fi

Zendia/Meta/fork-surface.sh "$remote/master"
