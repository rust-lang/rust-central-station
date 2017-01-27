#!/bin/sh

set -ex

docker build \
  --tag rust-central-station \
  --rm \
  .

exec docker run \
  --volume `pwd`/data:/src \
  --volume `pwd`/bin:/src/bin:ro \
  --volume `pwd`/data/letsencrypt:/etc/letsencrypt \
  --publish 80:80 \
  --publish 443:443 \
  --rm \
  rust-central-station
