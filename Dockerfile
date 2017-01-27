FROM ubuntu:16.04

RUN apt-get update -y && \
    apt-get install -y --no-install-recommends \
      gcc \
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
      cron

RUN curl https://sh.rustup.rs | sh -s -- -y
ENV PATH=$PATH:/root/.cargo/bin
RUN cargo install \
      --git https://github.com/alexcrichton/cancelbot \
      --rev 84587f1c3d80558f5a8302c2c6d551f214395aab \
      --debug

RUN git clone https://github.com/servo/homu /homu
RUN cd /homu && git reset --hard b82e98b628a2f8483f09b22ea75186b20b78cede
RUN pip3 install -e /homu

COPY tq /tmp/tq
RUN cargo install --path /tmp/tq
COPY rbars /tmp/rbars
RUN cargo install --path /tmp/rbars

ADD crontab /etc/cron.d/letsencrypt-renew
RUN chmod 0644 /etc/cron.d/letsencrypt-renew
RUN touch /var/log/cron.log

ENTRYPOINT ["/src/bin/run.sh"]
