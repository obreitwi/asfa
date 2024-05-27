#!/usr/bin/env bash

set -euo pipefail

toplevel="$(git rev-parse --show-toplevel)"

changelog="${toplevel}/CHANGELOG.md"

sed -i -e '4i ## Unreleased changes' "${changelog}"
sed -i -e '4G'  "${changelog}"

git add "${changelog}"
git commit -m "doc: add 'unreleased' section in changelog"
