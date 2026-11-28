# syntax=docker/dockerfile:1
# Multi-target build:
#   --target web      -> Nginx serving the React admin dashboard (proxies API to backend)
#   --target runtime  -> `rapi` Rust binary (default)

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

FROM nginxinc/nginx-unprivileged:1.27-alpine AS web
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=web-builder /web/dist /usr/share/nginx/html
EXPOSE 8080

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin rapi

WORKDIR /app
COPY --from=backend-builder /app/target/release/rapi /usr/local/bin/rapi
COPY config /app/config
RUN mkdir -p /app/data/uploads && chown -R rapi:rapi /app

USER rapi

ENV RAPI_ENV=production \
    RAPI_HOST=0.0.0.0 \
    RAPI_PORT=1337 \
    DATABASE_URL=sqlite:///app/data/rapi.db?mode=rwc \
    UPLOAD_DIR=/app/data/uploads \
    ENABLE_DOCS=false

EXPOSE 1337
CMD ["rapi", "serve"]
