#!/bin/bash

set -euxo pipefail

env \
  PROPTEST_DISABLE_FAILURE_PERSISTENCE=true \
  MIRIFLAGS='-Zmiri-env-forward=PROPTEST_DISABLE_FAILURE_PERSISTENCE' \
  bash -c 'cargo +nightly miri test'
