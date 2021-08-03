#!/usr/bin/env bash

# Usage:
#   print_changelog.sh VERSION
#
# Print changelog for specific version.

if (( $# > 0 )); then
    version="$1"
    shift 1
else
    echo "ERR: Need to specify version!" >&2
    exit 1
fi

sourcedir="$(dirname "$(readlink -m "${BASH_SOURCE[0]}")")"
path_changelog="${sourcedir}/../CHANGELOG.md"

check_existing() {
    local to_check
    to_check="$1"
    if ! which "${to_check}" > /dev/null; then
        echo "ERR: ${to_check} missing." >&2
        exit 1
    fi
}

check_existing awk

awk -v "version=v${version}" \
    -e '$1 ~ /^##$/ { if ($2 == version) { enabled = 1 } else {enabled = 0} }' \
    -e 'enabled { print }' \
    "${path_changelog}"
