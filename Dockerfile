# Build the server, then ship just the static binary on a slim runtime.
# SQLite is bundled into the binary (sqlx), so the runtime image needs no extras.
FROM rust:1-trixie AS build
WORKDIR /src
# Resolve and download the crate graph against the manifests alone, so a
# source-only change reuses this layer instead of re-fetching every dependency.
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml core/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
RUN mkdir -p core/src server/src \
    && touch core/src/lib.rs \
    && echo 'fn main() {}' > server/src/main.rs \
    && cargo fetch --locked \
    && rm -rf core/src server/src
COPY core ./core
COPY server ./server
RUN cargo build --release --locked -p yuiolink-server

FROM debian:trixie-slim
RUN useradd --create-home --uid 10001 app && mkdir -p /data && chown app:app /data
COPY --from=build /src/target/release/yuiolink-server /usr/local/bin/yuiolink-server
USER app
WORKDIR /data
# Bind on all interfaces so the fronting Caddy container can reach it; keep the
# DB on a writable path. Override YUIOLINK_BASE_URL at run time so returned links
# match the public HTTPS hostname.
ENV YUIOLINK_BIND=0.0.0.0:8080 \
    YUIOLINK_DB=/data/yuiolink.db
# The SQLite database (and its -wal/-shm sidecars) must outlive the container:
# without a volume, every `container rm` destroys all live links.
VOLUME /data
EXPOSE 8080
# No curl in the slim image; bash's /dev/tcp is enough to probe /healthz, which
# touches the database so a failed migration reads as unhealthy.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
    CMD bash -c "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /healthz HTTP/1.0\r\n\r\n' >&3 && grep -q ' 200 ' <&3"
ENTRYPOINT ["yuiolink-server"]
