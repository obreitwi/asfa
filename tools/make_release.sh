#!/bin/bash

# Use `cargo release` to publish a new version and update documentation. 

set -euo pipefail

toplevel="$(git rev-parse --show-toplevel)"

cd "${toplevel}"

cargo release "${@}"
"${toplevel}/tools/update-docs-latest-release.sh"
