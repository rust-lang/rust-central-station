#!/bin/bash

set -e

secrets=/data/secrets.toml

# We mounted /var/log from a local log dir, but ubuntu expects it to be owned by
# root:syslog, so change it here
#chown root:syslog /var/log
touch /var/log/cron.log

# Background daemons we use here
cron

export RUST_BACKTRACE=1

set -ex

# Generate an initial letsencrypt certificate if one isn't already available.
if [ -z "$DEV" ]; then
  if [ ! -d /etc/letsencrypt/renewal ]; then
    nginx -c /src/nginx.tmp.conf

    letsencrypt certonly \
        --webroot \
        --agree-tos \
        -m `tq nginx.email < $secrets` \
        -w /usr/share/nginx/html \
        -d `tq nginx.hostname < $secrets`,`tq nginx.hostname_alias < $secrets`

    nginx -s stop
  fi

  # Configure/run nginx
  rbars $secrets /src/nginx.conf.template > /tmp/nginx.conf
  nginx -c /tmp/nginx.conf
fi

# Import the GPG key that's specified in the secrets file
gpg --batch --import `tq dist.gpg-key < $secrets`

# Configure and run homu
rbars $secrets /src/homu.toml.template > /tmp/homu.toml
homu -v -c /tmp/homu.toml 2>&1 | logger --tag homu
