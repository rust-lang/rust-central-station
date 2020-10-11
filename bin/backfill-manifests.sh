#!/bin/bash
set -euo pipefail

secrets=/data/secrets.toml

declare minor_versions_with_patch_releases

minor_versions_with_patch_releases[12]=1
minor_versions_with_patch_releases[15]=1
minor_versions_with_patch_releases[22]=1
minor_versions_with_patch_releases[24]=1
minor_versions_with_patch_releases[26]=2
minor_versions_with_patch_releases[27]=2
minor_versions_with_patch_releases[29]=2
minor_versions_with_patch_releases[30]=1
minor_versions_with_patch_releases[31]=1
minor_versions_with_patch_releases[34]=2
minor_versions_with_patch_releases[41]=1
minor_versions_with_patch_releases[43]=1
minor_versions_with_patch_releases[44]=1
minor_versions_with_patch_releases[45]=2

export AWS_ACCESS_KEY_ID="$(tq dist.aws-access-key-id < $secrets)"
export AWS_SECRET_ACCESS_KEY="$(tq dist.aws-secret-key < $secrets)"

bucket="$(tq dist.upload-bucket < $secrets)"
dir="$(tq dist.upload-dir < $secrets)"

for minor in {8..47}
do
    if [ ${minor_versions_with_patch_releases[$minor]+_} ]; then
        last_patch=${minor_versions_with_patch_releases[$minor]};
    else
        last_patch=0;
    fi

    src="s3://${bucket}/${dir}/channel-rust-1.${minor}.${last_patch}.toml"
    dst="s3://${bucket}/${dir}/channel-rust-1.${minor}.toml"

    aws cp --only-show-errors "${src}" "${dst}"
    aws cp --only-show-errors "${src}".asc "${dst}".asc
    aws cp --only-show-errors "${src}".sha256 "${dst}".sha256
done
