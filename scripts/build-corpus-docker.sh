#!/bin/sh
# Build the thaum binary and Docker image for corpus execution tests.
#
# Usage: scripts/build-corpus-docker.sh
#
# Requires: cargo, docker

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Building thaum (release, with CLI)..."
cargo build --features cli --release --manifest-path "$PROJECT_ROOT/Cargo.toml"

echo "Building Docker image thaum-corpus-exec..."
cp "$PROJECT_ROOT/target/release/thaum" "$PROJECT_ROOT/tests/docker/thaum"
docker build -f "$PROJECT_ROOT/tests/docker/Dockerfile.corpus" \
    -t thaum-corpus-exec \
    "$PROJECT_ROOT/tests/docker"
rm "$PROJECT_ROOT/tests/docker/thaum"

echo "Done. Run corpus tests with: cargo test --test corpus"
