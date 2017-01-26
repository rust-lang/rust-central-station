#!/bin/bash

set -e

source /src/secrets.sh

/usr/sbin/rsyslogd

export RUST_BACKTRACE=1

cancelbot \
  --travis $travis_token \
  --appveyor $rust_appveyor_token \
  --appveyor-account rust-lang \
  --branch auto \
  --interval 60 \
  rust-lang/rust \
  2>&1 | logger --tag cancelbot-rust &

cancelbot \
  --travis $travis_token \
  --appveyor $cargo_appveyor_token \
  --appveyor-account rust-lang-libs \
  --branch auto-cargo \
  --interval 60 \
  rust-lang/cargo \
  2>&1 | logger --tag cancelbot-cargo &

sleep 3600
