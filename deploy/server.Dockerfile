# fm-server. Build context is the repo root (the server needs crates/fm-core).
FROM rust:1-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY beacons/linux ./beacons/linux
RUN cargo build --release --locked -p fm-server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home /data findmy \
 && mkdir -p /data && chown findmy /data
COPY --from=builder /src/target/release/fm-server /usr/local/bin/fm-server
USER findmy
ENV FM_BIND=0.0.0.0:3001 \
    FM_DATA_DIR=/data
VOLUME /data
EXPOSE 3001
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 CMD ["fm-server", "healthcheck"]
CMD ["fm-server"]
