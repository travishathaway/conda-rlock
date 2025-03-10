#!/bin/bash

set -euxo pipefail

export CARGO_PROFILE_RELEASE_STRIP=symbols
export CARGO_PROFILE_RELEASE_LTO=fat

export OPENSSL_DIR=$PREFIX

# Run the maturin build via pip which works for direct and
# cross-compiled builds.
$PYTHON -m pip install . -vv

cargo-bundle-licenses --format yaml --output THIRDPARTY.yml
