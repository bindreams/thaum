# syntax=docker/dockerfile:1
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev git
WORKDIR /src
COPY ./ ./
RUN git clean -fdX
RUN cargo build --features cli --release

FROM alpine:latest
RUN apk add --no-cache python3
COPY --from=builder /src/target/release/thaum /usr/local/bin/thaum
ENTRYPOINT ["thaum", "exec"]
