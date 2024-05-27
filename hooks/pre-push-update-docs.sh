#!/usr/bin/env bash

# NOTE: Currently disabled as we only generate/upload docs on release.
#
# Auto generate docs and upload them upon non-development pushes into the
# master branch.
#
# Needs to be put into .git/hook/pre-push

set -euo pipefail

remote="$1"
url="$2"

toplevel="$(git rev-parse --show-toplevel)"

if [[ "${url}" == "git@github.com:obreitwi/asfa"* ]]; then
    pushing_to_github=1
else
    pushing_to_github=0
fi

if (( pushing_to_github == 0)); then
    echo "DEBUG (pre-push hook): Not pushing to github master -> not updating docs." >&2
    exit 0
fi

pushing_release_version=0
pushing_to_master=0
while read local_ref local_sha remote_ref remote_sha
do
    echo "DEBUG (pre-push hook): Pushing: $local_ref $local_sha $remote_ref $remote_sha" >&2

    if [[ "$remote_ref" == "refs/heads/master" ]]; then
        pushing_to_master=1
    fi

    tmp_cargo_toml="$(mktemp)"
    if git show "${local_ref}:Cargo.toml" >"${tmp_cargo_toml}" 2>/dev/null; then
        if ! grep -q "^version.*-pre.*\"" "${tmp_cargo_toml}"; then
            pushing_release_version=1
        fi
    else
        pushing_release_version=0
    fi
    rm "${tmp_cargo_toml}"
done

if (( pushing_to_github == 1)) && (( pushing_to_master == 1)) && (( pushing_release_version == 1 )); then
    "${toplevel}/tools/update-docs-HEAD.sh" "${remote}"
fi

exit 0
