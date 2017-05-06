FROM ubuntu:16.04

RUN apt-get update -y && \
    apt-get install -y --no-install-recommends \
      g++ \
      curl \
      ca-certificates \
      libc6-dev \
      make \
      libssl-dev \
      pkg-config \
      python3-venv \
      python3-pip \
      python3-setuptools \
      git \
      rsyslog \
      nginx \
      letsencrypt \
      cron \
      ssh \
      gnupg \
      cmake \
      logrotate \
      file \
      ssmtp \
      locales

# Set the system locales
RUN locale-gen en_US.UTF-8
ENV LANG en_US.UTF-8
ENV LANGUAGE en_US:en
ENV LC_ALL en_US.UTF-8

# Install Rust and Cargo
RUN curl https://sh.rustup.rs | sh -s -- -y
ENV PATH=$PATH:/root/.cargo/bin

# Install cancelbot, a bot that cancels AppVeyor/Travis builds if we don't need
# them. This is how we keep a manageable queue on the two services
RUN cargo install \
      --git https://github.com/alexcrichton/cancelbot \
      --rev 9fc5ae5c5f2db6162541c00365932561421b25f2

# Install nag-rs, a bot for nagging the subteams
RUN cargo install \
      --git https://github.com/aturon/nag-rs \
      --rev 28e62bcaf33f34540551dda23714e0be11bb0d84

# Install homu, our integration daemon
RUN git clone https://github.com/servo/homu /homu
RUN cd /homu && git reset --hard b82e98b628a2f8483f09b22ea75186b20b78cede
RUN pip3 install -e /homu

# Install local programs used:
#
# * tq - a command line 'toml query' program, used to extract data from
#        secrets.toml
# * rbars - a command line program to run a handlebars file through a toml
#           configuration, in our case used to expand templates using the values
#           in secrets.toml
# * promote-release - cron job to download artifacts from travis/appveyor
#                     archives and publish them (also generate manifests)
COPY tq /tmp/tq
RUN cargo install --path /tmp/tq && rm -rf /tmp/tq
COPY rbars /tmp/rbars
RUN cargo install --path /tmp/rbars && rm -rf /tmp/rbars
COPY promote-release /tmp/promote-release
RUN cargo install --path /tmp/promote-release && rm -rf /tmp/promote-release

# Install commands used by promote-release binary. The awscli package is used to
# issue cloudfront invalidations. The `boto` package is a dependency of
# s3-directory-listing, and that's used to generate index.html files for our S3
# bucket.
RUN pip3 install awscli
RUN aws configure set preview.cloudfront true
RUN git clone https://github.com/brson/s3-directory-listing /s3-directory-listing
RUN pip3 install boto
RUN cd /s3-directory-listing && git reset --hard 1dc88c6b0f6c4df470d35d1c212ee65147926064

# Install s4cmd which claims to be faster than s3cmd
RUN pip3 install boto3 pytz
RUN git clone https://github.com/bloomreach/s4cmd /s4cmd
RUN cd /s4cmd && git reset --hard f5f5ff076507ae867843a17550fce11e05fb18fa
ENV PATH=$PATH:/s4cmd

# Install our crontab which runs our various services on timers
ADD crontab /etc/cron.d/rcs
RUN chmod 0644 /etc/cron.d/rcs

# And finally, initialize our known set of ssh hosts so git doesn't prompt us
# later.
RUN mkdir /root/.ssh && ssh-keyscan github.com >> /root/.ssh/known_hosts

CMD ["/src/bin/run.sh"]
