# syntax=docker/dockerfile:1
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev git
WORKDIR /src
COPY ./ ./
RUN git clean -fdX
RUN <<-EOF
	set -e
	cargo build --features cli --release
	cargo test --release --test corpus --features cli --no-run
	find target/release/deps -name 'corpus-*' -type f ! -name '*.d' \
		| head -1 | xargs -I{} cp {} target/release/corpus-test
EOF

FROM alpine:latest
RUN apk add --no-cache python3
COPY --from=builder /src/target/release/thaum /usr/local/bin/thaum
COPY --from=builder /src/target/release/corpus-test /usr/local/bin/corpus-test
COPY --from=builder /src/tests/corpus/ /src/tests/corpus/
