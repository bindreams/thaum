# syntax=docker/dockerfile:1
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev git
WORKDIR /src
COPY ./ ./
RUN git clean -fdx
RUN cargo build --features cli --release

FROM alpine:latest
COPY --from=builder /src/target/release/thaum /usr/local/bin/thaum
ENTRYPOINT ["thaum", "exec"]
