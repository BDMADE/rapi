# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim AS web-builder
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
RUN npm run build

FROM rust:1-bookworm AS backend-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release --bin rapi

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin rapi

WORKDIR /app
COPY --from=backend-builder /app/target/release/rapi /usr/local/bin/rapi
COPY --from=web-builder /web/dist /app/web/dist
COPY config /app/config
RUN mkdir -p /app/data/uploads && chown -R rapi:rapi /app

USER rapi

ENV RAPI_ENV=production \
    RAPI_HOST=0.0.0.0 \
    RAPI_PORT=1337 \
    DATABASE_URL=sqlite:///app/data/rapi.db?mode=rwc \
    UPLOAD_DIR=/app/data/uploads \
    WEB_DIST_DIR=/app/web/dist \
    ENABLE_DOCS=false

EXPOSE 1337
CMD ["rapi", "serve"]
