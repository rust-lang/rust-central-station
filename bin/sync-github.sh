#!/bin/bash
set -euo pipefail

export GITHUB_TOKEN="$(tq sync-github.token < /data/secrets.toml)"
export RUST_LOG=sync_github=debug
exec run-on-change https://team-api.infra.rust-lang.org/v1/teams.json sync-github --live
