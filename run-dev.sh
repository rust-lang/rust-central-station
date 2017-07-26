#!/bin/sh

set -ex

docker build \
  --tag rust-central-station \
  --rm \
  .

exec docker run \
  --volume `pwd`/data:/data \
  --volume `pwd`/data/letsencrypt:/etc/letsencrypt \
  --env DEV=1 \
  --publish 8080:80 \
  --rm \
  rust-central-station \
  "$@"
