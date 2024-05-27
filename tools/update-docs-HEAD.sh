#!/bin/bash

set -euo pipefail

remote="$1"

toplevel="$(git rev-parse --show-toplevel)"

check_dependency()
{
    to_check="$1"
    if ! which "${to_check}" >/dev/null; then
        echo "ERROR: ${to_check} not found!" >&2
        exit 1
    fi
}

check_all_dependencies() {
    check_dependency "ghp-import"
    check_dependency "cargo"
}

generate_docs()
{
    cd "${toplevel}"
    cargo doc --features=doc --no-deps
    # redirect from root folder to doc of asfa
    echo "<meta http-equiv=refresh content=0;url=asfa/index.html>" > "${toplevel}/target/doc/index.html"
}

upload_docs()
{
    git push "${remote}" :gh-pages || true
    git fetch "${remote}"
    git branch -D gh-pages || true
    ghp-import --force                       \
               --push                        \
               --no-jekyll                   \
               --remote "${remote}"          \
               --branch gh-pages             \
               --message="Generated rustdoc" \
        target/doc
}

check_all_dependencies
generate_docs
upload_docs
