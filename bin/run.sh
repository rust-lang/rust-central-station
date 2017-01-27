#!/bin/bash

set -e

secrets=/src/data/secrets.toml

/usr/sbin/rsyslogd
cron

export RUST_BACKTRACE=1

set -ex

# Generate an initial letsencrypt certificate if one isn't already available.
if [ ! -d /etc/letsencrypt/renewal ]; then
  nginx -c /src/nginx.tmp.conf

  letsencrypt certonly \
      --webroot \
      --agree-tos \
      -m `tq nginx.email < $secrets` \
      -w /usr/share/nginx/html \
      -d `tq nginx.hostname < $secrets`

  nginx -s stop
fi

# Configure/run nginx
rbars $secrets /src/nginx.conf.template > /tmp/nginx.conf
nginx -c /tmp/nginx.conf

gpg --import `tq dist.gpg-key < $secrets`

# Configure and run homu
rbars $secrets /src/homu.toml.template > /tmp/homu.toml
homu -v -c /tmp/homu.toml 2>&1 | logger --tag homu
