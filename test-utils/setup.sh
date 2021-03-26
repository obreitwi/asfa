#!/usr/bin/env bash

set -euo pipefail
shopt -s lastpipe

sourcedir="$(dirname "$(readlink -m "${BASH_SOURCE[0]}")")"


####################
# Helper functions #
####################

export_var() {
    local name="$1"
    local value="$2"

    eval "export ${name}=\"${value}\""
    echo "export ${name}=\"${value}\""
}

ensure_folder() {
    local folder="${1}"
    if [ ! -d "${folder}" ]; then
        mkdir -p "${folder}"
    fi
}


#################################
# Check if container is running #
#################################

if docker exec asfa-ci cat /config/test_root 2>/dev/null | read -r test_root; then
    container_set_up=1
else
    container_set_up=0
    test_root="$(mktemp --tmpdir -d tmp.asfa_ci.XXXXXXXXXX)"
fi


##########
# Config #
##########

ensure_folder "${test_root}"
export_var ASFA_TEST_ROOT "${test_root}"

folder_config="${test_root}/config"
ensure_folder "${folder_config}"

if (( container_set_up == 0 )); then
    cp "${sourcedir}/ci-config/raw.yaml" "${folder_config}/config.yaml"
fi
export_var ASFA_CONFIG "${folder_config}"


########################
# Create test-ssh keys #
########################

TEST_SSH_PRIVKEY_FILE="${ASFA_CONFIG}/test_key.pem"
TEST_SSH_PUBKEY_FILE="${TEST_SSH_PRIVKEY_FILE}.pub"

if (( container_set_up == 0 )); then
    ssh-keygen -t ed25519 -f "${TEST_SSH_PRIVKEY_FILE}" -m PEM -P "" >&2
    sed -i "s:TEST_SSH_PRIVKEY_FILE:${TEST_SSH_PRIVKEY_FILE}:" \
        "${ASFA_CONFIG}/config.yaml"

    ensure_folder "${HOME}/.ssh"

    if ! grep -q asfa-ci-key "${HOME}/.ssh/config"; then
        cat >>"${HOME}/.ssh/config" <<EOF 
Host asfa-ci-key
    Hostname localhost
    Port 2222
EOF
    fi
fi


#################
# Set up docker #
#################

export_var ASFA_FOLDER_UPLOAD "${test_root}/uploads"
ensure_folder "${ASFA_FOLDER_UPLOAD}"

folder_docker_config="${test_root}/docker-cfg"
ensure_folder "${folder_docker_config}"

if (( container_set_up == 0 )); then
    echo "${test_root}" > "${folder_docker_config}/test_root"

    docker build -t asfa-ci-image - >&2 <<EOF
FROM linuxserver/openssh-server
# needed for scp-functionality
RUN apk add --no-cache openssh-client
RUN apk add --no-cache at
RUN echo "asfa-ci-user" >> /etc/at.allow
EOF
    docker create                                         \
      --name=asfa-ci                                      \
      -e "PUID=$(id -u)"                                  \
      -e "PGID=$(id -g)"                                  \
      -e TZ=Europe/London                                 \
      -e "PUBLIC_KEY_FILE=${TEST_SSH_PUBKEY_FILE}"        \
      -e PASSWORD_ACCESS=true                             \
      -e USER_PASSWORD=foobar                             \
      -e USER_NAME=asfa-ci-user                           \
      -p 2222:2222                                        \
      -v "${ASFA_FOLDER_UPLOAD}:/var/www/default/uploads" \
      -v "${folder_docker_config}":/config                \
      --restart unless-stopped                            \
      asfa-ci-image >&2

    docker start asfa-ci >&2

    ensure_folder "${folder_docker_config}/.ssh"
    cat "${TEST_SSH_PUBKEY_FILE}" >> "${folder_docker_config}/.ssh/authorized_keys"
fi
