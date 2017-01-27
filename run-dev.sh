#!/bin/sh

set -ex

docker build \
  --tag rust-central-station \
  --rm \
  .

exec docker run \
  --volume `pwd`:/src \
  --publish 80:80 \
  --publish 443:443 \
  --rm \
  rust-central-station
