#!/bin/bash

# Switch to the project root to keep paths stable.
cd "$(dirname "$0")/.." || exit 1

cargo build

LOG_LEVEL=debug \
CLAUDE_CODE_ATTRIBUTION_HEADER=0 \
HTTP_PROXY=http://127.0.0.1:7890 \
HTTPS_PROXY=http://127.0.0.1:7890 \
./target/debug/copilot-api-proxy start --port 4143
