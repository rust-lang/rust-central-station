#!/bin/sh

set -ex

docker build \
  --tag rust-central-station \
  --rm \
  .

exec docker run \
  --volume `pwd`:/src:ro \
  --rm \
  rust-central-station
