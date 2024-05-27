#!/usr/bin/env bash

# Upate docs to latest released (i.e. tagged) version.

set -euo pipefail

# temporary worktree
tmp_wt="$(mktemp -d)"
toplevel="$(git rev-parse --show-toplevel)"

# get currently tracked remote
remote="$(git status -b --porcelain=v2 | awk '$2 ~ /branch.upstream/ { print $3 }' | cut -f 1 -d /)"

trap 'rm -rfv "${tmp_wt}"' EXIT

latest_version="$(git tag -l --sort=version:refname | tail -n 1)"

git worktree add "${tmp_wt}" "${latest_version}"

old_pwd="$PWD"

cd "${tmp_wt}"
"${toplevel}/tools/update-docs-HEAD.sh" "${remote}"

cd "${old_pwd}"
git worktree remove "${tmp_wt}"
