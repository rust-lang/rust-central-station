#!/bin/sh

set -e

secrets=/data/secrets.toml

exec cancelbot \
  --azure-pipelines-token `tq cancelbot.azure-pipelines-2-token < $secrets` \
  --azure-pipelines-org rust-lang2 \
  --branch auto \
  rust-lang/libc \
  rust-lang/stdarch
