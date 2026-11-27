import { Link, useParams } from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface ModuleSpec {
  title: string;
  phase: string;
  summary: string;
  tables: string[];
  capabilities: string[];
}

const SPECS: Record<string, ModuleSpec> = {
  "content-manager": {
    title: "Content Manager",
    phase: "Phase 3",
    summary:
      "Author, filter, localize, and publish entries across collection and single types using Strapi v5 Document Service semantics.",
    tables: ["documents", "content_versions", "content_relations", "ct_<uid>"],
    capabilities: [
      "Draft & Published row isolation per documentId and locale",
      "Dynamic form renderer with Zod validation, Tiptap Blocks JSON, and @dnd-kit Dynamic Zones",
      "Full-text search via SQLite FTS5 plus nested qs filter bar",
      "Immutable snapshot version history and side-by-side field diffing",
    ],
  },
  "content-type-builder": {
    title: "Content-Type Builder",
    phase: "Phase 2",
    summary:
      "Runtime schema modeling engine generating SQLite tables, indexes, and REST/GraphQL endpoints with zero server restarts.",
    tables: ["content_types", "field_definitions", "schema_migrations"],
    capabilities: [
      "Collection Types, Single Types, and reusable/repeatable Components",
      "All Strapi v5 scalar types, Media fields, Dynamic Zones, and bidirectional Relations",
      "Safe sea-query DDL generation and transactional 12-step SQLite table rebuilds on edits",
    ],
  },
  "media-library": {
    title: "Media Library",
    phase: "Phase 4",
    summary:
      "Asset storage, hierarchical folders, responsive format generation, and focal-point cropping.",
    tables: ["upload_files", "upload_folders", "upload_file_relations"],
    capabilities: [
      "Local filesystem storage by default with optional S3-compatible provider",
      "Background thumbnail/resize/WebP worker via Rust image crate",
      "Status-isolated media links so draft edits never mutate live published assets",
    ],
  },
  users: {
    title: "Roles, Users & Permissions",
    phase: "Phase 1",
    summary:
      "Granular RBAC matrix governing Admin Roles, End-User Roles, Public API access, and field-level conditions.",
    tables: ["admin_users", "admin_roles", "permissions", "sessions"],
    capabilities: [
      "Built-in Super Admin, Editor, and Author roles seeded at migration time",
      "Action + target UID + field-level permission evaluation cached in memory/Redis",
      "Argon2id password hashing and short-lived JWT + revocable refresh sessions",
    ],
  },
};

export function ModulePlaceholderPage({ moduleKey }: { moduleKey: string }) {
  const params = useParams();
  const key = moduleKey === "settings" ? "settings" : moduleKey;

  if (key === "settings") {
    const section = params.section ?? "general";
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-lg font-semibold text-zinc-900 capitalize">
              Settings: {section.replace(/-/g, " ")}
            </h1>
            <p className="mt-0.5 text-xs text-zinc-600">
              Platform governance, API tokens, webhooks, localization, and data transfer configuration.
            </p>
          </div>
          <Badge tone="indigo">Scheduled in Phases 1–6</Badge>
        </div>

        <div className="rounded-md border border-zinc-200 bg-white p-5 text-xs text-zinc-600">
          <p className="font-medium text-zinc-900">
            Underlying SQLite schema for <code className="font-mono">{section}</code> is already migrated.
          </p>
          <p className="mt-1.5">
            Tables <code className="font-mono">api_tokens</code>,{" "}
            <code className="font-mono">webhooks</code>,{" "}
            <code className="font-mono">locales</code>,{" "}
            <code className="font-mono">releases</code>,{" "}
            <code className="font-mono">workflow_stages</code>, and{" "}
            <code className="font-mono">audit_logs</code> are provisioned in{" "}
            <code className="font-mono">0001_init_core.sql</code> and ready for service wiring.
          </p>
        </div>
      </div>
    );
  }

  const spec = SPECS[key] ?? SPECS["content-manager"];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">{spec.title}</h1>
          <p className="mt-0.5 max-w-2xl text-xs text-zinc-600">{spec.summary}</p>
        </div>
        <Badge tone="indigo">{spec.phase}</Badge>
      </div>

      <div className="overflow-hidden rounded-md border border-zinc-200 bg-white">
        <div className="border-b border-zinc-200 px-4 py-3 flex items-center justify-between">
          <span className="text-xs font-semibold text-zinc-900">Provisioned SQLite tables</span>
          <div className="flex flex-wrap gap-1.5">
            {spec.tables.map((t) => (
              <Badge key={t} tone="mono">
                {t}
              </Badge>
            ))}
          </div>
        </div>
        <ul className="divide-y divide-zinc-200 text-xs text-zinc-700">
          {spec.capabilities.map((cap) => (
            <li key={cap} className="px-4 py-2.5">
              {cap}
            </li>
          ))}
        </ul>
      </div>

      <div>
        <Button variant="secondary" size="sm" asChild>
          <Link to="/">Return to system overview</Link>
        </Button>
      </div>
    </div>
  );
}
