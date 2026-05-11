#!/bin/bash

cargo build

HTTP_PROXY=http://127.0.0.1:7890 \
HTTPS_PROXY=http://127.0.0.1:7890 \
./target/debug/copilot-api-proxy start --port 4143 --verbose
