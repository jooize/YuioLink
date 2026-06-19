# Build the server, then ship just the static binary on a slim runtime.
# SQLite is bundled into the binary (sqlx), so the runtime image needs no extras.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY core ./core
COPY server ./server
RUN cargo build --release -p yuiolink-server

FROM debian:bookworm-slim
RUN useradd --create-home --uid 10001 app && mkdir -p /data && chown app:app /data
COPY --from=build /src/target/release/yuiolink-server /usr/local/bin/yuiolink-server
USER app
WORKDIR /data
# Bind on all interfaces so the fronting Caddy container can reach it; keep the
# DB on a writable path. Override YUIOLINK_BASE_URL at run time so returned links
# match the public HTTPS hostname.
ENV YUIOLINK_BIND=0.0.0.0:8080 \
    YUIOLINK_DB=/data/yuiolink.db
EXPOSE 8080
ENTRYPOINT ["yuiolink-server"]
