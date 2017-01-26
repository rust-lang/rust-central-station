FROM ubuntu:16.04

RUN apt-get update -y && \
    apt-get install -y --no-install-recommends \
      python3-venv \
      python3-pip \
      python3-setuptools \
      git && \
    git clone https://github.com/servo/homu /homu && \
    pyvenv .venv && \
    . .venv/bin/activate && \
    pip3 install -e homu
