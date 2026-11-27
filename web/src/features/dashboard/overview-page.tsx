import * as Tabs from "@radix-ui/react-tabs";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  Blocks,
  CheckCircle2,
  Database,
  ExternalLink,
  FolderOpen,
  Globe,
  RefreshCw,
  Server,
} from "lucide-react";
import { Link } from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { formatUptime } from "@/lib/utils";

export function OverviewPage() {
  const overviewQuery = useQuery({
    queryKey: ["system-overview"],
    queryFn: api.getSystemOverview,
    retry: 1,
  });

  const data = overviewQuery.data;
  const stats = data?.stats;

  const metrics = [
    {
      label: "Content types",
      value: stats?.contentTypes ?? 0,
      detail: "Collection & single schemas",
      icon: Blocks,
      to: "/content-type-builder",
    },
    {
      label: "Documents",
      value: stats?.documents ?? 0,
      detail: `${stats?.publishedDocuments ?? 0} published rows`,
      icon: Database,
      to: "/content-manager",
    },
    {
      label: "Media assets",
      value: stats?.mediaFiles ?? 0,
      detail: "Local provider active",
      icon: FolderOpen,
      to: "/media-library",
    },
    {
      label: "Active locales",
      value: stats?.locales ?? 1,
      detail: `Default: ${data?.defaultLocale ?? "en"}`,
      icon: Globe,
      to: "/settings/i18n",
    },
  ];

  return (
    <div className="space-y-6">
      {/* Page header */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">System overview</h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Strapi v5-compatible Rust Headless CMS runtime, schema registry, and API health.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => overviewQuery.refetch()}
            disabled={overviewQuery.isFetching}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${overviewQuery.isFetching ? "animate-spin" : ""}`} />
            Refresh telemetry
          </Button>
          <Button size="sm" asChild>
            <Link to="/content-type-builder">Define content type</Link>
          </Button>
        </div>
      </div>

      {overviewQuery.isError && (
        <div className="rounded-md border border-amber-300 bg-amber-50 px-4 py-3 text-xs text-amber-950">
          <p className="font-medium">Backend API unreachable on port 1337.</p>
          <p className="mt-1 text-amber-900/80">
            Start the Rust server with <code className="font-mono">cargo run --bin rapi -- serve</code> to
            populate live SQLite &amp; runtime metrics.
          </p>
        </div>
      )}

      {/* Connected 1px structural ledger grid */}
      <section className="grid grid-cols-1 divide-y divide-zinc-200 overflow-hidden rounded-md border border-zinc-200 bg-white sm:grid-cols-2 sm:divide-x sm:divide-y-0 lg:grid-cols-4">
        {metrics.map((m) => {
          const Icon = m.icon;
          return (
            <Link
              key={m.label}
              to={m.to}
              className="group flex flex-col justify-between p-4 transition-colors hover:bg-zinc-50/80"
            >
              <div className="flex items-center justify-between text-xs text-zinc-500">
                <span className="font-medium group-hover:text-zinc-900">{m.label}</span>
                <Icon className="h-4 w-4 text-zinc-400 group-hover:text-indigo-700" />
              </div>
              <div className="mt-3 flex items-baseline justify-between">
                <span className="tabular text-2xl font-semibold tracking-tight text-zinc-900">
                  {m.value}
                </span>
                <span className="text-[11px] text-zinc-500">{m.detail}</span>
              </div>
            </Link>
          );
        })}
      </section>

      {/* Two-column operational workspace */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        {/* Runtime diagnostics table */}
        <section className="overflow-hidden rounded-md border border-zinc-200 bg-white lg:col-span-7">
          <div className="flex items-center justify-between border-b border-zinc-200 px-4 py-3">
            <div className="flex items-center gap-2">
              <Server className="h-4 w-4 text-zinc-500" />
              <h2 className="text-xs font-semibold text-zinc-900">Runtime subsystems</h2>
            </div>
            <Badge tone="mono">axum 0.8 + sqlx</Badge>
          </div>

          <div className="divide-y divide-zinc-200 text-xs">
            <div className="flex items-center justify-between px-4 py-2.5">
              <span className="text-zinc-600">Primary database</span>
              <div className="flex items-center gap-2">
                <Badge tone="mono">SQLite ({data?.sqliteJournalMode ?? "WAL"})</Badge>
                <span className="text-zinc-500">1 writer / 8 readers</span>
              </div>
            </div>
            <div className="flex items-center justify-between px-4 py-2.5">
              <span className="text-zinc-600">Cache &amp; session store</span>
              <Badge tone={data?.cacheBackend === "redis" ? "indigo" : "emerald"}>
                {data?.cacheBackend === "redis" ? "Redis cluster/node" : "Moka in-memory (zero-config)"}
              </Badge>
            </div>
            <div className="flex items-center justify-between px-4 py-2.5">
              <span className="text-zinc-600">Background job queue</span>
              <div className="flex items-center gap-2">
                <span className="tabular text-zinc-500">{stats?.pendingJobs ?? 0} pending</span>
                <Badge tone="neutral">
                  {data?.queueBackend === "redis" ? "Redis queue" : "SQLite job_queue"}
                </Badge>
              </div>
            </div>
            <div className="flex items-center justify-between px-4 py-2.5">
              <span className="text-zinc-600">Process uptime</span>
              <span className="tabular font-mono text-zinc-800">
                {data ? formatUptime(data.uptimeSeconds) : "—"}
              </span>
            </div>
            <div className="flex items-center justify-between px-4 py-2.5">
              <span className="text-zinc-600">OpenAPI 3.1 schema</span>
              <a
                href="/openapi.json"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 font-mono text-indigo-700 hover:underline"
              >
                /openapi.json
                <ExternalLink className="h-3 w-3" />
              </a>
            </div>
          </div>
        </section>

        {/* Strapi v5 API Contract preview */}
        <section className="overflow-hidden rounded-md border border-zinc-200 bg-white lg:col-span-5">
          <Tabs.Root defaultValue="rest">
            <div className="flex items-center justify-between border-b border-zinc-200 px-4 py-2">
              <div className="flex items-center gap-2">
                <Activity className="h-4 w-4 text-zinc-500" />
                <h2 className="text-xs font-semibold text-zinc-900">Strapi v5 wire format</h2>
              </div>
              <Tabs.List className="inline-flex rounded border border-zinc-200 bg-zinc-100 p-0.5 text-[11px]">
                <Tabs.Trigger
                  value="rest"
                  className="rounded px-2 py-0.5 font-medium text-zinc-600 data-[state=active]:bg-white data-[state=active]:text-zinc-900 cursor-pointer"
                >
                  REST v5
                </Tabs.Trigger>
                <Tabs.Trigger
                  value="graphql"
                  className="rounded px-2 py-0.5 font-medium text-zinc-600 data-[state=active]:bg-white data-[state=active]:text-zinc-900 cursor-pointer"
                >
                  GraphQL v5
                </Tabs.Trigger>
              </Tabs.List>
            </div>

            <Tabs.Content value="rest" className="p-4">
              <p className="mb-2 text-xs text-zinc-600">
                Flattened document responses with <code className="font-mono">documentId</code> and full{" "}
                <code className="font-mono">qs</code> filter/populate support:
              </p>
              <pre className="overflow-x-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] leading-relaxed text-zinc-100">
{`GET /api/articles?filters[slug][$eq]=hello&populate=*

{
  "data": [
    {
      "id": 1,
      "documentId": "k9p4x7m2n8q1r5t3v6w0y2z4",
      "title": "Hello from Rust",
      "locale": "en",
      "publishedAt": "2026-11-27T12:00:00Z"
    }
  ],
  "meta": {
    "pagination": { "page": 1, "pageSize": 25, "total": 1 }
  }
}`}
              </pre>
            </Tabs.Content>

            <Tabs.Content value="graphql" className="p-4">
              <p className="mb-2 text-xs text-zinc-600">
                Exact Strapi v5 query naming (<code className="font-mono">&lt;plural&gt;</code>,{" "}
                <code className="font-mono">&lt;plural&gt;_connection</code>,{" "}
                <code className="font-mono">&lt;singular&gt;(documentId)</code>):
              </p>
              <pre className="overflow-x-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] leading-relaxed text-zinc-100">
{`query ArticlesList {
  articles_connection(status: PUBLISHED, locale: "en") {
    nodes {
      documentId
      title
    }
    pageInfo {
      page
      total
    }
  }
}`}
              </pre>
            </Tabs.Content>
          </Tabs.Root>
        </section>
      </div>

      {/* Phase status strip */}
      <section className="rounded-md border border-zinc-200 bg-white p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-xs">
            <CheckCircle2 className="h-4 w-4 text-emerald-600" />
            <span className="font-medium text-zinc-900">Phase 0 foundation active</span>
            <span className="text-zinc-500">
              Core platform tables migrated, split SQLite WAL pools online, Strapi qs parser &amp; error envelopes verified.
            </span>
          </div>
          <Badge tone="indigo">Next: Phase 1 Auth, RBAC &amp; API Tokens</Badge>
        </div>
      </section>
    </div>
  );
}
