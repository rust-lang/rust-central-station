#!/bin/sh

set -ex

docker pull alexcrichton/rust-central-station

exec docker run \
  --volume `pwd`:/src \
  --publish 80:80 \
  --publish 443:443 \
  --rm \
  --detach \
  alexcrichton/rust-central-station
