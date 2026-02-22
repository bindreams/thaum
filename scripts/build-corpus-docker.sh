#!/bin/sh
# Build the Docker image for corpus execution tests.
#
# Usage: scripts/build-corpus-docker.sh
#
# Requires: docker

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Building Docker image thaum-corpus-exec..."
docker build -f "$PROJECT_ROOT/tests/docker/Dockerfile.corpus" \
    -t thaum-corpus-exec \
    "$PROJECT_ROOT"

echo "Done. Run corpus tests with: cargo test --test corpus"
