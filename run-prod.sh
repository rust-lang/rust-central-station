#!/bin/sh

set -ex

docker pull alexcrichton/rust-central-station

mkdir -p data/logs/nginx
exec docker run \
  --volume `pwd`/data:/data \
  --volume `pwd`/data/letsencrypt:/etc/letsencrypt \
  --volume `pwd`/data/logs:/var/log \
  --publish 80:80 \
  --publish 443:443 \
  --rm \
  --detach \
  alexcrichton/rust-central-station
