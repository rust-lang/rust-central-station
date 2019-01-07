#!/bin/sh
set -e

$(aws ecr get-login --no-include-email --region us-west-1)
docker tag rust-central-station:latest 890664054962.dkr.ecr.us-west-1.amazonaws.com/rust-central-station:latest
docker push 890664054962.dkr.ecr.us-west-1.amazonaws.com/rust-central-station:latest
