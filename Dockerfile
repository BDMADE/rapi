# syntax=docker/dockerfile:1
FROM rust:1.85-bookworm AS backend-builder
WORKDIR /app
COPY Cargo.toml ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release --bin rapi

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates sqlite3 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /app/target/release/rapi /usr/local/bin/rapi
RUN mkdir -p /app/data/uploads
ENV RAPI_HOST=0.0.0.0 \
    RAPI_PORT=1337 \
    DATABASE_URL=sqlite:///app/data/rapi.db?mode=rwc \
    UPLOAD_DIR=/app/data/uploads
EXPOSE 1337
CMD ["rapi", "serve"]
