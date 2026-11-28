# RAPI — Rust Headless CMS (Strapi v5 Feature Parity)

RAPI is a local-first, high-performance Headless CMS built with **Rust (Axum + SQLite WAL)** and **React + TypeScript (Vite + Tailwind + Radix UI)**, designed for API and functional parity with **Strapi v5** without a Node.js backend runtime.

## Highlights

- **Zero-config single binary:** `cargo run --bin rapi -- serve` boots immediately using SQLite (`WAL` mode: 1 writer + 8 readers), in-memory `moka` cache, and a persistent SQLite `job_queue`.
- **Optional Redis scaling:** Set `REDIS_URL=redis://127.0.0.1:6379` to transparently switch `CacheBackend`, `QueueBackend`, and `PubSubBackend` to Redis.
- **Strapi v5 wire compatibility:** Flattened `{ data, meta }` responses with 24-character `documentId`s, `serde_qs` nested bracket queries (`filters`, polymorphic `populate`, `fields`, `sort`, `pagination`, `locale`, `status`), and exact Strapi v5 GraphQL query naming.
- **Extensibility & AI:** Native `CmsPlugin` trait + fuel-metered `wasmi` WebAssembly plugin sandbox, Custom Fields registry, OIDC Single Sign-On, Opt-in AI Editorial Assistant, and built-in JSON-RPC 2.0 MCP server (`POST /api/mcp` and `rapi mcp` stdio).
- **Precision Admin UI:** Compact, keyboard-first (`⌘K`) React dashboard wired to live platform telemetry.

## Workspace Layout

```text
rapi/
├─ crates/
│  ├─ cms-core/        # DocumentId, AppConfig, Strapi v5 envelopes, Cache/Queue/PubSub traits
│  ├─ cms-plugin-api/  # CmsPlugin trait & registry (native Rust + Phase 7 WASM plugins)
│  ├─ cms-db/          # Split SQLite WAL pools, migrations, SQLite queue, Redis runtime
│  ├─ cms-services/    # ServiceContainer & system/domain services
│  ├─ cms-api/         # Axum 0.8 router, qs parser, OpenAPI 3.1 (/openapi.json)
│  ├─ cms-worker/      # Background queue runner (embedded or standalone)
│  └─ cms-bin/         # `rapi` CLI binary (serve | worker | migrate | seed | info | mcp)
├─ migrations/         # Core SQLite platform tables
└─ web/                # Vite + React 19 + TypeScript + Tailwind v4 + Radix UI dashboard
```

## Quick Start (Local Zero-Config)

### 1. Start the Rust CMS Backend

```bash
cp config/.env.example .env
cargo run --bin rapi -- serve
```

The API server starts on `http://localhost:1337` (with automatic SQLite database creation & core migrations in `./data/rapi.db`):

- `GET http://localhost:1337/health` — Liveness check
- `GET http://localhost:1337/ready` — Readiness check (SQLite WAL + Cache backend)
- `GET http://localhost:1337/admin/init` — Onboarding status (`hasAdmin`, `defaultLocale`)
- `GET http://localhost:1337/admin/system/overview` — Platform counts & runtime diagnostics
- `GET http://localhost:1337/openapi.json` — OpenAPI 3.1 specification

### 2. Start the React Admin Dashboard

```bash
cd web
npm install
npm run dev
```

Open `http://localhost:5173` (Vite proxies `/api`, `/admin`, `/health`, `/ready`, and `/openapi.json` to `http://127.0.0.1:1337`).

## Docker Compose (with Redis)

```bash
docker compose up --build
```

## Running Tests

```bash
# Rust unit + API integration tests
cargo test --workspace

# Frontend strict TypeScript & production bundle verification
cd web && npm run build
```
