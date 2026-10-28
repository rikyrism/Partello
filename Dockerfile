# syntax=docker/dockerfile:1.7
# Partello: one image serving the API, WebSockets and the built web app.

FROM node:22-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.94-slim-bookworm AS server
WORKDIR /server
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/migrations ./migrations
COPY server/src ./src
# openapi.yaml is embedded into the binary at compile time.
COPY docs/openapi.yaml /docs/openapi.yaml
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 partello
WORKDIR /app
COPY --from=server /server/target/release/partello-server /app/partello-server
COPY --from=web /web/dist /app/web
ENV HOST=0.0.0.0 PORT=8080 STATIC_DIR=/app/web LOG_FORMAT=json
USER partello
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=3s --retries=3 CMD ["/app/partello-server", "--healthcheck"]
CMD ["/app/partello-server"]
