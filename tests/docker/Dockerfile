FROM alpine:latest
RUN apk add --no-cache dash bash
COPY conformance_runner.sh /conformance_runner.sh
RUN chmod +x /conformance_runner.sh
ENTRYPOINT ["/conformance_runner.sh"]
