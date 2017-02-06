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
      s3cmd \
      cmake \
      logrotate \
      file

RUN curl https://sh.rustup.rs | sh -s -- -y
ENV PATH=$PATH:/root/.cargo/bin
RUN cargo install \
      --git https://github.com/alexcrichton/cancelbot \
      --rev 9fc5ae5c5f2db6162541c00365932561421b25f2

RUN git clone https://github.com/servo/homu /homu
RUN cd /homu && git reset --hard b82e98b628a2f8483f09b22ea75186b20b78cede
RUN pip3 install -e /homu

COPY tq /tmp/tq
RUN cargo install --path /tmp/tq && rm -rf /tmp/tq
COPY rbars /tmp/rbars
RUN cargo install --path /tmp/rbars && rm -rf /tmp/rbars
COPY promote-release /tmp/promote-release
RUN cargo install --path /tmp/promote-release && rm -rf /tmp/promote-release

RUN pip3 install awscli
RUN aws configure set preview.cloudfront true

RUN git clone https://github.com/brson/s3-directory-listing \
      /s3-directory-listing \
      --rev 1dc88c6b0f6c4df470d35d1c212ee65147926064

ADD crontab /etc/cron.d/letsencrypt-renew
RUN chmod 0644 /etc/cron.d/letsencrypt-renew
RUN touch /var/log/cron.log
RUN mkdir /root/.ssh && ssh-keyscan github.com >> /root/.ssh/known_hosts

ENTRYPOINT ["/src/bin/run.sh"]
