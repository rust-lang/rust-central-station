#!/bin/sh

set -ex

docker pull alexcrichton/rust-central-station

mkdir -p /var/log/rcs/nginx
exec docker run \
  --volume `pwd`:/src:ro \
  --volume `pwd`/data:/src/data \
  --volume `pwd`/data/letsencrypt:/etc/letsencrypt \
  --volume /var/log/rcs:/var/log \
  --publish 80:80 \
  --publish 443:443 \
  --rm \
  --detach \
  alexcrichton/rust-central-station
