#!/bin/sh

set -e

secrets=/data/secrets.toml

exec cancelbot \
  --azure-pipelines-token `tq cancelbot.azure-pipelines-token < $secrets` \
  --azure-pipelines-org rust-lang \
  --branch auto \
  rust-lang-ci/rust
