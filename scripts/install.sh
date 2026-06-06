#!/bin/bash

# Switch to the project root to keep paths stable.
cd "$(dirname "$0")/.." || exit 1

cargo install --path .