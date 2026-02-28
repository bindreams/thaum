# syntax=docker/dockerfile:1
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY ./ ./
RUN <<-EOF
	set -e
	cargo build --features cli --release
	cargo bench --bench bench --no-run
	find target/release/deps -name 'bench-*' -type f ! -name '*.d' \
		| head -1 | xargs -I{} cp {} target/release/bench
EOF

FROM alpine:latest
RUN <<-EOF
	apk add --no-cache bash dash valgrind
	wget -qO /tmp/hyperfine.tar.gz \
		https://github.com/sharkdp/hyperfine/releases/download/v1.19.0/hyperfine-v1.19.0-x86_64-unknown-linux-musl.tar.gz
	tar xzf /tmp/hyperfine.tar.gz -C /usr/local/bin --strip-components=1 \
		hyperfine-v1.19.0-x86_64-unknown-linux-musl/hyperfine
	rm /tmp/hyperfine.tar.gz
EOF
COPY --from=builder /src/target/release/thaum /usr/local/bin/thaum
COPY --from=builder /src/target/release/bench /usr/local/bin/bench
