FROM alpine:latest

RUN mkdir -p /opt/docker/bin
COPY target/x86_64-unknown-linux-musl/release/jsaas /opt/docker/bin/start

FROM scratch
COPY --from=0 /opt/docker /opt/docker
WORKDIR /
ENTRYPOINT ["/opt/docker/bin/start"]
CMD []
