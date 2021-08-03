#!/bin/bash

# Use `cargo release` to publish a new version and update documentation.
# Also use `make-binary-release.sh` to push binary release.

set -euo pipefail

toplevel="$(git rev-parse --show-toplevel)"

cd "${toplevel}"

cargo release "${@}"
"${toplevel}/tools/update-docs-latest-release.sh"
"${toplevel}/tools/make-binary-release.sh"
