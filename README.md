# Rust Central Station

Or otherwise just another name for the old buildmaster.

This repo is hooked up to an automated docker build

* https://hub.docker.com/r/alexcrichton/rust-central-station/

On the destination machine you can run it as:

    ./run-prod.sh

Services currently provided are:

* cancelbot for rust-lang/rust
* cancelbot for rust-lang/cargo
* homu
* nginx in front of homu
* ssl via letsencrypt

Future services

* signing Rust releases

## Architecture

This is intended to be run as a container on the destination server, so the
container here specifies everything about what's being run.

* Secrets are stored in `secrets.toml` next to `secrets.toml.example` and are
  shared with the container.
* Programs are provided in the container (`tq` and `rbars`) which will read the
  TOML configuration for use in shell scripts.
* Everything pipes output to `logger` to collect output
* Services are just run as simple daemons, not a lot of management.
