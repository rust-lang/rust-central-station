#!/bin/sh

set -e

secrets=/data/secrets.toml

exec cancelbot \
  --travis `tq cancelbot.travis-token < $secrets` \
  --appveyor `tq cancelbot.rust-appveyor-token < $secrets` \
  --appveyor-account rust-lang \
  --azure-pipelines-token `tq cancelbot.azure-pipelines-token < $secrets` \
  --branch auto \
  rust-lang/rust
