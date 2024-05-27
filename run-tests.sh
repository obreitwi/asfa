#!/usr/env/bin bash

source <(bash ./test-utils/setup.sh)
cargo test --verbose
