FROM ubuntu:16.04

RUN apt-get update -y && \
    apt-get install -y --no-install-recommends \
      gcc \
      curl \
      ca-certificates \
      libc6-dev \
      make \
      libssl-dev \
      pkg-config && \
    curl https://sh.rustup.rs | sh -s -- -y && \
    /root/.cargo/bin/cargo install \
      --root /usr/local \
      --git https://github.com/alexcrichton/cancelbot && \
    /root/.cargo/bin/rustup self uninstall -y

ENV RUST_BACKTRACE=1
CMD cancelbot \
      --appveyor-account rust-lang \
      --branch auto \
      --interval 60 \
      rust-lang/rust
