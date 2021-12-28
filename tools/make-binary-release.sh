#!/bin/bash
#
# Create a binary release.
#
# Usage:
#    make_binary_release.sh [TARGET_FOLDER]

set -euo pipefail

if (( $# > 0 )); then
    folder_target="$1"
    shift 1
else
    folder_target="$PWD"
fi

check_existing() {
    local to_check
    to_check="$1"
    if ! which "${to_check}" > /dev/null; then
        echo "ERR: ${to_check} missing." >&2
        exit 1
    fi
}
check_existing awk
check_existing cargo
check_existing find
check_existing help2man
check_existing gh

tmp_build="$(mktemp -d)"

rm_tmp_build() {
    rm -rf "${tmp_build}"
}

trap rm_tmp_build EXIT

toplevel="$(git rev-parse --show-toplevel)"
version="$(git tag -l --sort=version:refname | tail -n 1 | tr -d v)" 
target=x86_64-unknown-linux-gnu
release="asfa-v${version}-${target}"
folder_release="${tmp_build}/${release}"
folder_man="${folder_release}/man/man1"
release_archive="${folder_target}/${release}.tar.gz"
path_bin="${toplevel}/target/${target}/release/asfa"

cd "${toplevel}"

RUSTFLAGS=-Ctarget-cpu=x86-64 cargo build --release --frozen --target=${target}

mkdir -p "${folder_release}"
mkdir -p "${folder_target}"
mkdir -p "${folder_man}"

install -Dm755 "${path_bin}" "${folder_release}"

help2man -o "${folder_man}/asfa.1" "${path_bin}"

# Generate info about all subcommands except for 'help' (which leads to error)
"${path_bin}" --help | awk 'enabled && $1 != "help" { print $1 } /^SUBCOMMANDS:$/ { enabled=1 }' \
    | while read -r cmd; do
    help2man \
        "--version-string=${version}" \
        -o "${folder_man}/asfa-${cmd}.1" \
        "${path_bin} $cmd"
done
find "${folder_man}" -type f -print0 | xargs -0 gzip

cp -a "${toplevel}/example-config" "${folder_release}/example-config"
find "${folder_release}/example-config" -type f -print0 | xargs -0 chmod 644

include_file() {
    install -Dm644 "${toplevel}/$1" "${folder_release}"
}

include_file LICENSE-MIT
include_file LICENSE-APACHE
include_file README.md
include_file CHANGELOG.md

cd "${folder_release}/.."

echo "Creating: ${release_archive}"
echo
tar cfvz "${release_archive}" "${release}"

cd "${toplevel}"
echo "Create github release for ${release_archive}? (y/n)"
read -r line
if [ "${line}" == y ]; then
    gh release create "v${version}" "${release_archive}" -F <(${toplevel}/tools/changelog_version.sh "${version}")
else
    echo "Aborting.." >&2
fi
