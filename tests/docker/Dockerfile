# syntax=docker/dockerfile:1
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev git
WORKDIR /src
COPY ./ ./
RUN git clean -fdX
RUN <<-EOF
	set -e
	cargo build --features cli --release
	cargo test --release --test gauntlet --features cli --no-run
	find target/release/deps -name 'gauntlet-*' -type f ! -name '*.d' \
		| head -1 | xargs -I{} cp {} target/release/gauntlet-test
EOF

FROM alpine:latest
RUN apk add --no-cache python3
COPY --from=builder /src/target/release/thaum /usr/local/bin/thaum
COPY --from=builder /src/target/release/gauntlet-test /usr/local/bin/gauntlet-test
COPY --from=builder /src/tests/gauntlet/ /src/tests/gauntlet/
