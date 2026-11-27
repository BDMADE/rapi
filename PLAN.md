# RAPI — Headless CMS (Strapi v5 Feature Parity) — Build Plan

**Goal:** Build a headless CMS with the full Strapi v5 feature set, using:

- **Backend:** Rust (Axum + Tokio), SQLite3 (`sqlx` + `sea-query`), pluggable Cache/Queue/PubSub (zero-config local default + Redis adapter)
- **Frontend:** React + TypeScript (Vite), minimal-but-complete admin dashboard
- **No Node.js backend, no Strapi dependency.** Strapi v5 is the functional and API contract reference, not a runtime.

---

## 1. Guiding principles

1. **Feature parity, simpler UI.** Every Strapi v5 capability exists (content types, components, dynamic zones, i18n, draft/publish + history, RBAC, tokens, media, REST + GraphQL, webhooks, email, preview, releases, workflows, audit logs, plugins, custom fields, MCP). The dashboard is compact, high-density, and neutral instead of decorative.
2. **Dynamic schema at runtime.** Content types are metadata in SQLite; the engine generates/alters tables via `sea-query` ASTs and drives generic CRUD, filtering, validation, and API generation. Adding a content type requires **zero redeploys** — same as Strapi's Content-Type Builder.
3. **Strapi v5-compatible API shapes.**
   - REST responses follow v5's flattened format (`{ data: { documentId, ...fields }, meta }` — no `.attributes` wrapper), `qs` bracket query syntax (`filters`, polymorphic `populate`, `fields`, `sort`, `pagination`, `locale`, `status`), relation mutation operators (`connect` with positional `before`/`after`/`start`/`end`, `disconnect`, `set`), and v5 error format.
   - GraphQL uses Strapi v5's exact query/mutation naming (`<pluralApiId>`, `<pluralApiId>_connection`, `<singularApiId>(documentId: ID!)`, etc.).
   - Rich text supports both Strapi v5 **`blocks`** (structured JSON AST compatible with `@strapi/blocks-react-renderer`) and **`richtext`** (Markdown).
4. **Zero-config single-binary local DX, horizontally scalable with Redis.**
   - Running `rapi serve` works out of the box with **zero external services**: SQLite file + in-memory cache (`moka`) + SQLite-backed `job_queue` + Tokio `broadcast` pub/sub.
   - Setting `REDIS_URL` transparently upgrades `CacheBackend`, `QueueBackend`, and `PubSubBackend` to Redis for multi-instance deployments, distributed locks, rate limiting, and high-throughput workers.
5. **SQLite-safe architecture.** Split connection pools: single-connection serialized writer (`max_connections = 1`) + multi-connection reader pool with `WAL` mode, `synchronous = NORMAL`, `foreign_keys = ON`, and `busy_timeout = 5000ms`. Keep repository traits clean so PostgreSQL can be added later without touching services.
6. **Everything local-first.** Local file storage and SMTP by default; S3/mail-provider adapters are optional feature flags.

---

## 2. Technology choices

| Concern | Choice | Why |
|---|---|---|
| HTTP framework | **Axum 0.8 + Tokio + Tower** | Typed handlers, composable middleware, OpenAPI via `utoipa` |
| Database & Dynamic SQL | **SQLite via `sqlx` + `sea-query`** | Runtime queries (`sqlx`) paired with safe AST-based DDL & dynamic `SELECT`/`WHERE` generation (`sea-query`) to prevent SQL injection on dynamic identifiers |
| Query string parser | **`serde_qs` + custom Strapi `qs` AST** | Parses nested bracket queries (`filters[$and][0][title][$containsi]=x&populate[author][fields][0]=name`) and polymorphic `populate` |
| Cache / queues / pub-sub | **Traits (`moka` + SQLite queue default; `redis` async adapter)** | Zero-config dev startup; production-grade distributed cache, streams, locks, and pub/sub when `REDIS_URL` is configured |
| Auth | **`argon2` + `jsonwebtoken`** | JWT access tokens, refresh tokens in session store, Argon2id hashed passwords |
| GraphQL | **`async-graphql` (`dynamic-schema` feature) + `async-graphql-axum`** | Runtime schema built via `async_graphql::dynamic::*` matching Strapi v5 naming; delegates to the same service layer as REST |
| Validation | Server: schema rule engine + `garde`; Client: **Zod** (generated from schema) | Per-field required/min/max/regex/unique/enum rules |
| Rich text | **Tiptap** (with Strapi v5 `blocks` JSON serializer) + Markdown editor | Supports both `blocks` JSON AST and classic `richtext` Markdown |
| Drag & drop | **`@dnd-kit`** | Dynamic-zone block reordering, relation ordering, media grid sorting |
| Email | **`lettre`** (SMTP), provider trait | Password reset, user emails, test email, webhook notifications |
| Images | **`image` crate** + optional `libvips` feature | Thumbnails, resize, crop, format conversion, metadata extraction |
| Webhooks | **`reqwest` + HMAC-SHA256 signing + retry worker** | Signed deliveries, delivery log, exponential backoff |
| Search | **SQLite FTS5** | Full-text search over entries + media filenames |
| Plugins (Phase 7) | **`CmsPlugin` Rust trait + WASM (`wasmtime` / Extism)** | Compile-time native Rust plugins + sandboxed runtime-installable Marketplace plugins |
| Docs | **`utoipa` + Swagger UI** | Generated OpenAPI for REST/admin APIs |
| Observability | **`tracing` + `tracing-subscriber`** (JSON/pretty); `/health`, `/ready` | Structured operability |
| Frontend build | **Vite + React 19 + TypeScript (strict)** | Fast, strictly typed |
| Frontend data | **TanStack Query v5**, **Zustand** for UI state | Server cache + minimal global UI state |
| Frontend UI | **Tailwind CSS + Radix primitives + `lucide-react`** | Crisp, high-density neutral design system, accessible primitives |
| Forms | **React Hook Form + Zod** | Dynamic forms generated from content-type schema |
| Testing | Rust: `cargo test` + temp SQLite; Frontend: Vitest + Playwright | Unit, API contract, E2E |

---

## 3. Repository structure

```
rapi/
├─ Cargo.toml                  # Cargo workspace
├─ crates/
│  ├─ cms-core/                # domain models, DocumentId, config, error types, cache/queue traits, DTOs
│  ├─ cms-db/                  # split SQLite pools, core migrations, sea-query DDL & dynamic query builder
│  ├─ cms-services/            # auth, RBAC, content, schema, media, i18n, versions, webhooks, email, search
│  ├─ cms-api/                 # Axum routers: REST, GraphQL, admin API, qs parser, OpenAPI, middleware
│  ├─ cms-worker/              # background job consumers (webhooks, media, email, releases, FTS index)
│  ├─ cms-plugin-api/          # CmsPlugin trait, WASM host interface, hooks, registries
│  └─ cms-bin/                 # `rapi serve|worker|migrate|seed` CLI binary
├─ web/                        # React + TS dashboard (Vite)
│  └─ src/{app,features,components,lib,styles}
├─ migrations/                 # sqlx core-schema migrations
├─ config/                     # default config + .env.example
├─ docker-compose.yml          # app + Redis (optional profile / full stack)
├─ PLAN.md
└─ README.md
```

---

## 4. Core data model (SQLite)

### 4.1 Platform tables (migrations, versioned)

- `admin_users` (`id`, `email`, `username`, `password_hash`, `firstname`, `lastname`, `is_active`, `is_super_admin`, `last_login_at`, timestamps)
- `admin_roles`, `admin_permissions`, `admin_user_roles`
- `permissions` (`id`, `subject_type`, `subject_id`, `action`, `target_uid`, `conditions_json`, `fields_json`, `properties_json`) — shared model for admin roles, user roles, API tokens, and public role
- `api_tokens` (`id`, `name`, `description`, `token_type` [`full-access`|`read-only`|`custom`], `access_key_hash`, `expires_at`, `last_used_at`, timestamps)
- `admin_tokens` / `sessions` (refresh token hashes, expiry, user agent, IP)
- `content_types` (`uid`, `kind` [`collectionType`|`singleType`|`component`], `category`, `singular_name`, `plural_name`, `display_name`, `description`, `icon`, `draft_and_publish`, `i18n_enabled`, `plugin_options_json`, `options_json`, timestamps)
- `field_definitions` (`id`, `content_type_uid`, `name`, `field_type`, `required`, `unique_field`, `localized`, `position`, `default_value_json`, `validation_json`, `relation_json`, `component_json`, `custom_field_uid`)
- `schema_migrations` (`id`, `version`, `content_type_uid`, `operation`, `ddl_sql`, `applied_at`) — audit/idempotency log of runtime DDL changes
- `documents` (`id`, `document_id` [24-char alphanumeric cuid2/nanoid], `content_type_uid`, `locale`, `status` [`draft`|`published`], `version`, `published_at`, `created_by`, `updated_by`, timestamps) — **authoritative cross-type document index** updated in the same transaction as `ct_<uid>` for polymorphic lookups, global search, releases, and workflows
- `content_versions` (`id`, `document_id`, `content_type_uid`, `version`, `locale`, `status`, `data_json`, `created_by`, `created_at`) — immutable snapshots for content history & diff
- `content_relations` (`id`, `from_content_type_uid`, `from_document_id`, `from_status` [`draft`|`published`], `from_locale`, `field_name`, `component_path`, `to_content_type_uid`, `to_document_id`, `position`) — status-isolated ordered relations (including component-nested relations)
- `locales` (`id`, `code`, `name`, `is_default`, `created_at`, `updated_at`)
- `upload_folders`, `upload_files`, `upload_file_relations` (`from_document_id`, `from_status`, `from_locale`, `field_name`, `component_path`, `file_id`, `position`)
- `webhooks` + `webhook_deliveries` (`events_json`, `headers_json`, `url`, `secret`, `is_enabled`, delivery status, HTTP code, attempts)
- `audit_logs` (`id`, `actor_type`, `actor_id`, `action`, `target_uid`, `target_id`, `ip`, `user_agent`, `payload_json`, `created_at`)
- `releases`, `release_entries`, `workflow_stages`, `workflow_entries`
- `email_templates`, `job_queue` (fallback persistent queue when Redis is disabled, plus dead-letter history)
- `plugin_registry`, `custom_fields`

### 4.2 Content tables (generated by Content-Type Builder)

For each collection/single type, `SchemaEngine` creates `ct_<sanitized_uid>`:

- **System columns:** `id INTEGER PRIMARY KEY AUTOINCREMENT`, `document_id TEXT NOT NULL`, `locale TEXT NOT NULL DEFAULT 'en'`, `status TEXT NOT NULL DEFAULT 'draft'`, `version INTEGER NOT NULL DEFAULT 1`, `published_at TEXT`, `created_at TEXT NOT NULL`, `updated_at TEXT NOT NULL`, `created_by TEXT`, `updated_by TEXT`
- Unique composite index on `(document_id, locale, status)`
- One column per scalar field; `blocks`, `json`, repeatable/single components, and dynamic zones stored as validated JSON columns (with `__component` discriminators matching Strapi v5)
- Indexes on `(status, published_at)`, `(locale, status)`, and filterable/unique fields; FTS5 virtual table `fts_<sanitized_uid>` for searchable text fields
- Relation and media fields are resolved via `content_relations` / `upload_file_relations` (matching target `status`), never leaked as internal join rows

**Publishing model (Strapi v5 Document Service):**
- Every logical entry has a stable 24-char `documentId` across all locales and statuses.
- Saving edits writes to the `status = 'draft'` row (and `from_status = 'draft'` relations).
- Publishing copies scalar fields + `draft` relations atomically into the `status = 'published'` row (`published_at = now`), increments `version`, and writes a full snapshot to `content_versions`.
- Content API reads default to `status = 'published'`; `?status=draft` reads draft rows (subject to permissions).

**Schema evolution:**
- Field add → `ALTER TABLE ADD COLUMN`.
- Field type change / rename / drop constraint → transactional SQLite 12-step table rebuild (`CREATE TABLE ct_new` → `INSERT INTO ct_new SELECT ...` → `DROP TABLE` → `ALTER TABLE RENAME`) recorded in `schema_migrations`.

---

## 5. Feature parity checklist (Strapi v5 → RAPI)

| Strapi v5 feature | RAPI implementation | Phase |
|---|---|---|
| Admin panel / login / onboarding | React shell, first-admin registration, JWT + session refresh | 0–1 |
| Content-Type Builder (collection/single/component, all field types, relations, advanced settings) | Metadata-driven `SchemaEngine` + visual editor UI | 2 |
| Components + repeatable components + Dynamic Zones | JSON columns + component registry + `@dnd-kit` editor | 2–3 |
| Content Manager (list, FTS search, filters, sort, pagination, status filter) | `sea-query` dynamic repo + REST endpoint + dense table UI | 3 |
| Entry edit view, conditional fields, on-the-fly relation creation | Dynamic forms from schema, Zod rules, relation picker, `blocks` + `richtext` | 3 |
| Draft & Publish, versions, content history | Document/status split, publish/unpublish, snapshot diff sidebar | 3 / 5 |
| REST Content API (full `qs` query syntax & `connect`/`disconnect`/`set`) | Axum routes + `serde_qs` AST + `data`/`meta` v5 format | 3 |
| GraphQL API (Strapi v5 naming) | `async_graphql::dynamic`, `<plural>`, `<plural>_connection`, `<singular>(documentId)` | 6 |
| Users & Permissions (end users) | `end_users` table, `/api/auth/local/*`, user roles | 1 / 6 |
| RBAC (admin roles + field/condition-level permissions) | Permission engine in middleware + matrix UI | 1 |
| API Tokens + Admin Tokens (scoped, expiry) | Hashed DB tokens, cached lookup, token UI | 1 |
| Media Library (upload, folders, crop/resize, providers) | Local disk provider + optional S3, `image` worker, UI | 4 |
| Internationalization (built-in in v5) | `locales`, localized fields, locale switcher, per-locale versions/dynamic zones | 5 |
| Preview (basic + live) | Preview URL config + split/iframe panel; SSE live refresh | 5 |
| Webhooks (events, HMAC signing, retries, logs) | Background worker + delivery log UI + test trigger | 6 |
| Email plugin | `lettre` SMTP + templates + test email UI | 6 |
| Data management / export-import | JSON/CSV export-import, schema export-import, Strapi v5 archive importer | 6 |
| Custom fields | Backend field-type registry + frontend widget registry | 7 |
| Plugin system & Marketplace | Compile-time `CmsPlugin` trait + sandboxed WASM (`wasmtime`/Extism) runtime plugins | 7 |
| Releases | Release groups + scheduled atomic publish worker | 5 |
| Review workflows | Customizable stages + assignments + permission gates | 5 |
| Audit logs | Audit middleware on all admin mutations + filterable UI | 5 |
| SSO | OIDC via `openidconnect` | 7 |
| AI features (opt-in) | OpenAI-compatible adapter for field translation & draft generation | 7 |
| MCP server | Built-in JSON-RPC MCP endpoint/stdio exposing content, schema, and media tools | 7 |

---

## 6. Backend architecture

### 6.1 Layers & Runtime Backends

```
HTTP (Axum 0.8) ──▶ Middleware (auth, RBAC, rate limit, qs parser, audit, CORS, tracing)
                        │
                        ▼
                  Services (cms-services: content, schema, media, auth, i18n, version, webhook...)
                        │
         ┌──────────────┴───────────────────────┐
         ▼                                      ▼
   Repositories (cms-db: sqlx + sea-query)   Runtime Backends (CacheBackend / QueueBackend / PubSubBackend)
         │                                      ├─ Default: Moka (memory) + SQLite job_queue + Tokio broadcast
     SQLite (WAL: 1 writer + N readers)         └─ Optional: Redis (when REDIS_URL is set)
```

### 6.2 REST API contract (Strapi v5-compatible)

```
GET    /api/{pluralApiId}?filters[field][$eq]=x&populate=*&fields[0]=title&sort[0]=createdAt:desc&pagination[page]=1&pagination[pageSize]=25&locale=en&status=published
GET    /api/{pluralApiId}/{documentId}
POST   /api/{pluralApiId}                 ?status=draft|published
PUT    /api/{pluralApiId}/{documentId}    ?status=draft|published
DELETE /api/{pluralApiId}/{documentId}
GET    /api/{singularApiId}               / PUT /api/{singularApiId} / DELETE /api/{singularApiId}
POST   /api/upload                        GET /api/upload/files, /api/upload/files/{id}
POST   /api/auth/local                    /register  /forgot-password  /reset-password
```

Response shape:
```json
{
  "data": {
    "id": 1,
    "documentId": "clkgylmcc000008lcdd868feh",
    "title": "Hello RAPI",
    "locale": "en",
    "publishedAt": "2026-11-27T12:00:00Z",
    "createdAt": "2026-11-27T11:55:00Z",
    "updatedAt": "2026-11-27T12:00:00Z"
  },
  "meta": {
    "pagination": { "page": 1, "pageSize": 25, "pageCount": 1, "total": 1 }
  }
}
```
Error shape:
```json
{
  "data": null,
  "error": {
    "status": 400,
    "name": "ValidationError",
    "message": "Invalid input",
    "details": { "errors": [] }
  }
}
```

### 6.3 GraphQL (Strapi v5 naming)

Built dynamically at runtime with `async_graphql::dynamic`:
- Queries: `<pluralApiId>(filters, sort, pagination, locale, status)`, `<pluralApiId>_connection(...) { nodes { ... } pageInfo { page pageSize pageCount total } }`, `<singularApiId>(documentId: ID!, locale, status)`
- Mutations: `create<PascalSingular>(data, status, locale)`, `update<PascalSingular>(documentId: ID!, data, status, locale)`, `delete<PascalSingular>(documentId: ID!, locale)`
- Custom scalars: `JSON`, `DateTime`, `Date`, `Time`, `Long`, `Upload`
- Hot-swapped via `ArcSwap<Schema>` whenever `SchemaEngine` mutates content types (invalidated across nodes via `PubSubBackend`).

### 6.4 Cache, Queue & Pub/Sub Key Namespace

| Namespace | Purpose |
|---|---|
| `rapi:schema:<uid>` | Serialized content-type & field definitions |
| `rapi:permissions:<subject>` | Resolved permission sets |
| `rapi:token:<hash>` | API/Admin token → `Actor` lookup |
| `rapi:session:<id>` | Refresh token sessions |
| `rapi:rl:<ip\|actor>` | Sliding-window rate limit counters |
| `rapi:jobs:{webhook,media,email,release,index}` | Async background jobs |
| `rapi:pubsub:invalidations` | Cross-instance schema/GraphQL/permission cache invalidation |
| `rapi:lock:release:<id>` | Distributed scheduled-publish locks |

---

## 7. Frontend plan (Precision Administrative UI)

### 7.1 Design direction

- **Precision utilitarian aesthetic:** Crisp zinc/slate surface hierarchy (`zinc-950`/`zinc-50`), **1px structural hairline borders**, tabular numerals (`tabular-nums`) for counts/timestamps/IDs, monospace badges for `documentId`, API UIDs, HTTP methods, and locales.
- **Single purposeful accent:** Indigo (`indigo-600` / `indigo-500`) reserved strictly for primary actions, active states, and focus rings; semantic emerald/amber/rose pills for `Published` / `Draft` / `Modified` / `Error`.
- **Layout:** Fixed 240px left navigation rail (collapsible, grouped into Content, Media, Builder, Access, Settings), 48px top command header (breadcrumbs, environment/health indicator, `⌘K` quick jump, locale & theme toggle), and dense workspace canvas.
- **Reusable primitives (Radix + Tailwind):** Button, Input, Badge, Card, Dialog, DropdownMenu, Select, Tabs, Switch, Checkbox, Tooltip, Toast, DataTable, EmptyState.

### 7.2 Routes / screens

| Route | Screen |
|---|---|
| `/` | System overview: content type metrics, document counts, storage, runtime status (SQLite + Cache/Queue backend), quick actions |
| `/login`, `/onboarding`, `/forgot-password` | Auth & first-admin setup flow |
| `/content-manager` & `/content-manager/:uid` | Collection/Single type browser, filter bar, status tabs, locale selector, bulk actions |
| `/content-manager/:uid/:documentId` | Two-column entry editor (schema-driven form + right inspector rail for status, `documentId`, versions, locales, preview) |
| `/content-type-builder` | Content types, components, visual field builder, relation configurator, JSON schema inspector |
| `/media-library` | Asset grid/table, folder tree, upload dropzone, focal crop/resize, metadata drawer |
| `/users`, `/roles`, `/admin/users`, `/admin/roles` | End-user & Admin RBAC management with permission matrix |
| `/settings/*` | API tokens, Admin tokens, Webhooks, i18n Locales, Email, Upload, Preview, Releases, Workflows, Audit Logs, Plugins, Custom Fields, Data Transfer |

---

## 8. Implementation phases & milestones

Each phase ends with runnable software + automated tests.

- [x] **Plan Review & Architecture Alignment**
- [x] **Phase 0 — Foundation (repo, backend skeleton, runtime backends, design system)**
  - Cargo workspace (`cms-core`, `cms-db`, `cms-services`, `cms-api`, `cms-worker`, `cms-plugin-api`, `cms-bin`), layered config/env, Strapi v5 error envelope, `tracing`, `/health` & `/ready` & `/api` info endpoints.
  - Split SQLite WAL pools + core platform migrations (`0001_init_core.sql`), pluggable `CacheBackend` / `QueueBackend` / `PubSubBackend` (in-memory + SQLite default, Redis adapter when `REDIS_URL` is set).
  - `docker-compose.yml` (app + optional Redis), `.env.example`, `README.md`.
  - Vite + React + TypeScript strict app, Tailwind CSS + Radix UI primitives, app shell (sidebar, topbar, command palette trigger), overview dashboard wired to live backend health/status API, login/onboarding skeleton.
  - ✅ Milestone: `cargo test` passes, `cargo run --bin rapi -- serve` + `npm run build`/`dev` both run cleanly with zero external dependencies (and connect to Redis when configured); `/health` and `/ready` return structured status.
- [x] **Phase 1 — Auth, RBAC, tokens**
- [x] **Phase 2 — Schema engine & Content-Type Builder**
- [x] **Phase 3 — Content Manager + REST API**
- [ ] **Phase 4 — Media Library**
- [ ] **Phase 5 — i18n, preview, governance**
- [ ] **Phase 6 — GraphQL, integrations, data management**
- [ ] **Phase 7 — Extensibility (WASM plugins, MCP, OIDC, AI) & parity polish**

---

## 9. Testing strategy

- **Unit:** `sea-query` SQL/DDL generation, `qs` query parser, permission matching, version diffing, field validation.
- **Integration:** Temp SQLite (and optional Redis); spin up Axum router with `tower::ServiceExt`; assert Strapi v5 JSON shapes and error codes.
- **Contract:** Golden JSON fixtures for REST/GraphQL responses, filters, populate, pagination, and `connect`/`disconnect`/`set`.
- **E2E (Playwright):** Onboarding → create content type → create entry → publish → query REST/GraphQL → i18n → media → webhook log.
