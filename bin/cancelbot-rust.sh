#!/bin/sh

set -e

secrets=/data/secrets.toml

exec cancelbot \
  --azure-pipelines-token `tq cancelbot.azure-pipelines-token < $secrets` \
  --branch auto \
  rust-lang/rust
