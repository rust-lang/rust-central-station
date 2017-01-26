#!/bin/sh

set -ex

docker pull alexcrichton/rust-central-station
exec docker run \
  --volume `pwd`:/src:ro \
  --rm \
  alexcrichton/rust-central-station
