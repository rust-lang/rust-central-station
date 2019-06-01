FROM ubuntu:18.04

RUN apt-get update -y && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      g++ \
      curl \
      ca-certificates \
      libc6-dev \
      make \
      libssl-dev \
      pkg-config \
      python \
      python3-venv \
      python3-pip \
      python3-setuptools \
      git \
      nginx \
      letsencrypt \
      cron \
      ssh \
      gnupg \
      cmake \
      logrotate \
      file \
      ssmtp \
      locales \
      zlib1g-dev

# Set the system locales
RUN locale-gen en_US.UTF-8
ENV LANG en_US.UTF-8
ENV LANGUAGE en_US:en
ENV LC_ALL en_US.UTF-8

# Install Rust and Cargo
RUN curl https://sh.rustup.rs | sh -s -- -y
ENV PATH=$PATH:/root/.cargo/bin

# Install homu, our integration daemon
RUN git clone https://github.com/rust-lang/homu /homu && \
    cd /homu && git reset --hard 4bea22d27d2ea86d821f0bf8a45da9a8cb36dfb1
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
# * run-on-change - a command line program to run a command only when the
#                   content of a web page changes
# * sync-mailgun - a command line program to synchronize Mailgun mailing lists
#                  with the team repo
# * cancelbot - bot that cancels AppVeyor/Travis builds if we don't need them.
#               This is how we keep a manageable queue on the two services
COPY tq /tmp/tq
RUN cargo install --path /tmp/tq && rm -rf /tmp/tq
COPY rbars /tmp/rbars
RUN cargo install --path /tmp/rbars && rm -rf /tmp/rbars
COPY promote-release /tmp/promote-release
RUN cargo install --path /tmp/promote-release && rm -rf /tmp/promote-release
COPY run-on-change /tmp/run-on-change
RUN cargo install --path /tmp/run-on-change && rm -rf /tmp/run-on-change
COPY sync-mailgun /tmp/sync-mailgun
RUN cargo install --path /tmp/sync-mailgun && rm -rf /tmp/sync-mailgun
COPY cancelbot /tmp/cancelbot
RUN cargo install --path /tmp/cancelbot && rm -rf /tmp/cancelbot

# Install commands used by promote-release binary. The awscli package is used to
# issue cloudfront invalidations.
RUN pip3 install awscli
RUN aws configure set preview.cloudfront true

# Install our crontab which runs our various services on timers
ADD crontab /etc/cron.d/rcs
RUN chmod 0644 /etc/cron.d/rcs

# Initialize our known set of ssh hosts so git doesn't prompt us later.
RUN mkdir /root/.ssh && ssh-keyscan github.com >> /root/.ssh/known_hosts

# Copy the source directory into the image so we can run scripts and template
# configs from there
COPY . /src/

CMD ["/src/bin/run.sh"]
