import { useQuery } from "@tanstack/react-query";
import {
  CheckCircle2,
  ChevronRight,
  ExternalLink,
  Layers,
  Pencil,
  Plus,
  RotateCcw,
  Server,
  User,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useUiStore } from "@/app/store";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { useAuthStore } from "@/features/auth/auth-store";
import { api, type ContentDocument, type ContentTypeSchema } from "@/lib/api";
import { cn, formatUptime } from "@/lib/utils";

type WidgetId = "last-edited" | "last-published" | "profile" | "entries" | "runtime";

interface WidgetDefinition {
  id: WidgetId;
  title: string;
  description: string;
  defaultVisible: boolean;
}

const WIDGET_DEFINITIONS: WidgetDefinition[] = [
  {
    id: "last-edited",
    title: "Last edited entries",
    description: "Quickly jump back into your most recently modified collection and single-type entries.",
    defaultVisible: true,
  },
  {
    id: "last-published",
    title: "Last published entries",
    description: "Monitor live content entries most recently published across your content types.",
    defaultVisible: true,
  },
  {
    id: "profile",
    title: "Profile",
    description: "View your administrator identity, assigned RBAC roles, and profile shortcut.",
    defaultVisible: true,
  },
  {
    id: "entries",
    title: "Entries & assets summary",
    description: "Overview of total content types, documents, media files, and active locales.",
    defaultVisible: true,
  },
  {
    id: "runtime",
    title: "System & runtime status",
    description: "Live Rust Axum uptime, SQLite WAL status, cache backend, and OpenAPI spec.",
    defaultVisible: false,
  },
];

const STORAGE_TOUR_CLOSED_KEY = "rapi_home_guided_tour_closed";
const STORAGE_WIDGETS_KEY = "rapi_home_active_widgets";
const STORAGE_DEPLOY_DONE_KEY = "rapi_home_task_deploy_done";

interface NormalizedEntryRow {
  key: string;
  title: string;
  schemaUid: string;
  schemaLabel: string;
  isSingleType: boolean;
  documentId: string;
  status: "draft" | "published" | "modified" | "none";
  updatedAt: string;
  publishedAt: string | null;
  editTo: string;
}

function formatRelativeTime(isoString?: string | null): string {
  if (!isoString) return "Just now";
  const ts = Date.parse(isoString);
  if (Number.isNaN(ts)) return "Recently";
  const diffSec = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (diffSec < 45) return "Just now";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} minute${diffMin === 1 ? "" : "s"} ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr} hour${diffHr === 1 ? "" : "s"} ago`;
  const diffDays = Math.floor(diffHr / 24);
  if (diffDays < 30) return `${diffDays} day${diffDays === 1 ? "" : "s"} ago`;
  const diffMonths = Math.floor(diffDays / 30);
  return `${diffMonths} month${diffMonths === 1 ? "" : "s"} ago`;
}

function extractEntryTitle(doc: ContentDocument, schema: ContentTypeSchema): string {
  const preferredKeys = ["title", "name", "headline", "subject", "label", "slug", "username", "email"];
  for (const key of preferredKeys) {
    const val = doc[key];
    if (typeof val === "string" && val.trim().length > 0) {
      return val.trim();
    }
  }
  for (const field of schema.fields ?? []) {
    if (field.fieldType === "string" || field.fieldType === "uid" || field.fieldType === "email") {
      const val = doc[field.name];
      if (typeof val === "string" && val.trim().length > 0) {
        return val.trim();
      }
    }
  }
  if (schema.kind === "singleType") {
    return schema.displayName;
  }
  return `${schema.displayName} #${doc.documentId.slice(0, 6)}`;
}

function resolvePublicationBadge(
  doc: ContentDocument,
  schema: ContentTypeSchema,
): "draft" | "published" | "modified" | "none" {
  if (!schema.draftAndPublish) {
    return "none";
  }
  if (doc.publicationState === "modified") return "modified";
  if (doc.publicationState === "published" || doc.publishedAt) return "published";
  return "draft";
}

function EmptyEntriesIllustration({ message }: { message: string }) {
  return (
    <div className="flex flex-col items-center justify-center py-10 text-center">
      <svg
        width="88"
        height="72"
        viewBox="0 0 88 72"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
        className="mb-3"
      >
        {/* Soft background motion lines */}
        <rect x="8" y="25" width="22" height="3.5" rx="1.75" fill="#EAEAEF" />
        <rect x="4" y="34" width="26" height="3.5" rx="1.75" fill="#DCDCE4" />
        <rect x="13" y="43" width="18" height="3.5" rx="1.75" fill="#EAEAEF" />
        <rect x="60" y="31" width="19" height="3.5" rx="1.75" fill="#EAEAEF" />
        <rect x="62" y="40" width="14" height="3.5" rx="1.75" fill="#F0F0FF" />

        {/* Back document sheet */}
        <rect
          x="26"
          y="16"
          width="30"
          height="38"
          rx="3"
          fill="#F0F0FF"
          stroke="#9795FF"
          strokeWidth="1.4"
        />

        {/* Front document sheet with folded corner */}
        <path
          d="M34 12H54L62 20V50C62 51.6569 60.6569 53 59 53H34C32.3431 53 31 51.6569 31 50V15C31 13.3431 32.3431 12 34 12Z"
          fill="#FFFFFF"
          stroke="#7B79FF"
          strokeWidth="1.5"
        />
        <path
          d="M54 12V18C54 19.1046 54.8954 20 56 20H62"
          stroke="#7B79FF"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {/* Document text lines */}
        <rect x="37" y="26" width="19" height="2.2" rx="1.1" fill="#A5A4FF" />
        <rect x="37" y="32" width="19" height="2.2" rx="1.1" fill="#C0BFFF" />
        <rect x="37" y="38" width="15" height="2.2" rx="1.1" fill="#C0BFFF" />
        <rect x="37" y="44" width="11" height="2.2" rx="1.1" fill="#D9D8FF" />
      </svg>
      <p className="text-[13px] font-normal text-[#666687]">{message}</p>
    </div>
  );
}

function WavingHandBadge() {
  return (
    <div className="mb-4 inline-flex h-10 w-10 items-center justify-center">
      <svg
        width="34"
        height="34"
        viewBox="0 0 36 36"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
      >
        {/* Motion arcs */}
        <path
          d="M7.5 13.5C6.8 11.2 7.5 8.6 9.4 7"
          stroke="#7B79FF"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <path
          d="M25.5 7.5C27.5 8.8 28.6 11.1 28.3 13.5"
          stroke="#7B79FF"
          strokeWidth="2"
          strokeLinecap="round"
        />
        {/* Friendly Cobalt Hand */}
        <path
          d="M18.6 6.8C19.5 6.5 20.5 7 20.8 7.9L23.2 15.1L24.8 12.6C25.4 11.7 26.6 11.5 27.4 12.1C28.2 12.7 28.4 13.8 27.8 14.7L24.2 21.8C22.5 25.2 18.6 27.2 14.9 26.2C11.1 25.2 8.7 21.6 9.2 17.7L10.3 10.6C10.5 9.6 11.4 9 12.3 9.2C13.2 9.4 13.8 10.3 13.6 11.2L12.9 15.7L13.5 7.9C13.6 6.9 14.5 6.2 15.4 6.3C16.3 6.4 17 7.2 16.9 8.2L16.4 14.4L17.4 8.1C17.5 7.4 18 6.9 18.6 6.8Z"
          fill="#4945FF"
        />
      </svg>
    </div>
  );
}

export function OverviewPage() {
  const user = useAuthStore((s) => s.user);
  const activeLocale = useUiStore((s) => s.activeLocale);

  const [tourClosed, setTourClosed] = useState<boolean>(() => {
    try {
      return localStorage.getItem(STORAGE_TOUR_CLOSED_KEY) === "true";
    } catch {
      return false;
    }
  });

  const [deployVisited, setDeployVisited] = useState<boolean>(() => {
    try {
      return localStorage.getItem(STORAGE_DEPLOY_DONE_KEY) === "true";
    } catch {
      return false;
    }
  });

  const [activeWidgets, setActiveWidgets] = useState<WidgetId[]>(() => {
    try {
      const raw = localStorage.getItem(STORAGE_WIDGETS_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as WidgetId[];
        if (Array.isArray(parsed) && parsed.length > 0) {
          return parsed;
        }
      }
    } catch {
      // ignore corrupted storage
    }
    return WIDGET_DEFINITIONS.filter((w) => w.defaultVisible).map((w) => w.id);
  });

  const [widgetModalOpen, setWidgetModalOpen] = useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_TOUR_CLOSED_KEY, String(tourClosed));
    } catch {
      // ignore
    }
  }, [tourClosed]);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_WIDGETS_KEY, JSON.stringify(activeWidgets));
    } catch {
      // ignore
    }
  }, [activeWidgets]);

  const overviewQuery = useQuery({
    queryKey: ["system-overview"],
    queryFn: api.getSystemOverview,
    retry: 1,
  });

  const schemasQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
    retry: 1,
  });

  const apiTokensQuery = useQuery({
    queryKey: ["tokens", "api"],
    queryFn: () => api.listTokens("api"),
    retry: 1,
  });

  const contentTypes = schemasQuery.data?.contentTypes ?? [];

  // Fetch recent entries across content types to populate Last Edited & Last Published widgets
  const recentEntriesQuery = useQuery({
    queryKey: [
      "home-recent-entries",
      activeLocale,
      contentTypes.map((c) => c.uid).join(","),
    ],
    enabled: contentTypes.length > 0,
    queryFn: async (): Promise<NormalizedEntryRow[]> => {
      const targetSchemas = contentTypes.slice(0, 12);
      const results = await Promise.all(
        targetSchemas.map(async (schema) => {
          try {
            if (schema.kind === "singleType") {
              const doc = await api.getSingleEntry(schema.uid, activeLocale);
              if (!doc) return [];
              const row: NormalizedEntryRow = {
                key: `${schema.uid}:single`,
                title: extractEntryTitle(doc, schema),
                schemaUid: schema.uid,
                schemaLabel: "Single-Type",
                isSingleType: true,
                documentId: doc.documentId ?? "single",
                status: resolvePublicationBadge(doc, schema),
                updatedAt: doc.updatedAt,
                publishedAt: doc.publishedAt,
                editTo: `/content-manager/${encodeURIComponent(schema.uid)}/single`,
              };
              return [row];
            }

            const res = await api.listEntries(schema.uid, {
              page: 1,
              pageSize: 6,
              locale: activeLocale,
              sort: "updatedAt:desc",
            });
            return (res.data ?? []).map((doc): NormalizedEntryRow => ({
              key: `${schema.uid}:${doc.documentId}`,
              title: extractEntryTitle(doc, schema),
              schemaUid: schema.uid,
              schemaLabel: schema.displayName,
              isSingleType: false,
              documentId: doc.documentId,
              status: resolvePublicationBadge(doc, schema),
              updatedAt: doc.updatedAt,
              publishedAt: doc.publishedAt,
              editTo: `/content-manager/${encodeURIComponent(schema.uid)}/${encodeURIComponent(doc.documentId)}`,
            }));
          } catch {
            return [];
          }
        }),
      );

      return results.flat();
    },
  });

  const allEntries = recentEntriesQuery.data ?? [];

  const lastEditedEntries = useMemo(() => {
    return [...allEntries]
      .sort((a, b) => Date.parse(b.updatedAt || "") - Date.parse(a.updatedAt || ""))
      .slice(0, 4);
  }, [allEntries]);

  const lastPublishedEntries = useMemo(() => {
    return allEntries
      .filter((item) => Boolean(item.publishedAt))
      .sort(
        (a, b) => Date.parse(b.publishedAt || b.updatedAt || "") - Date.parse(a.publishedAt || a.updatedAt || ""),
      )
      .slice(0, 4);
  }, [allEntries]);

  const stats = overviewQuery.data?.stats;
  const hasSchemas = (stats?.contentTypes ?? contentTypes.length) > 0;
  const hasPublishedContent = (stats?.publishedDocuments ?? lastPublishedEntries.length) > 0;
  const hasApiTokens = (apiTokensQuery.data?.length ?? 0) > 0;

  const tasks = [
    {
      id: "schema",
      label: "Create your schema",
      done: hasSchemas,
      cta: "Start",
      to: "/content-type-builder",
      external: false,
    },
    {
      id: "content",
      label: "Create and publish content",
      done: hasPublishedContent,
      cta: "Start",
      to: "/content-manager",
      external: false,
    },
    {
      id: "token",
      label: "Copy an API token",
      done: hasApiTokens,
      cta: "Start",
      to: "/settings/api-tokens",
      external: false,
    },
    {
      id: "deploy",
      label: "Deploy your application to Cloud",
      done: deployVisited,
      cta: "Read documentation",
      to: "/openapi.json",
      external: true,
    },
  ];

  const completedTasksCount = tasks.filter((t) => t.done).length;
  const progressPercent = Math.round((completedTasksCount / tasks.length) * 100);

  const toggleWidget = (id: WidgetId) => {
    setActiveWidgets((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  };

  const displayName = user?.firstname?.trim() || user?.username?.trim() || "Admin";
  const fullName = [user?.firstname, user?.lastname].filter(Boolean).join(" ") || displayName;

  const markDeployRead = () => {
    setDeployVisited(true);
    try {
      localStorage.setItem(STORAGE_DEPLOY_DONE_KEY, "true");
    } catch {
      // ignore
    }
  };

  return (
    <div className="space-y-7">
      {/* Top Greeting Header */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-[28px] leading-[1.22] font-bold tracking-[-0.02em] text-[#32324d]">
            Hello {displayName}
          </h1>
          <p className="mt-1.5 text-[15px] text-[#666687]">
            Welcome to your administration panel
          </p>
        </div>

        <div className="flex items-center gap-2.5">
          {tourClosed && (
            <button
              type="button"
              onClick={() => setTourClosed(false)}
              className="inline-flex h-9 items-center gap-1.5 rounded-[4px] border border-[#dcdce4] bg-white px-3.5 text-xs font-semibold text-[#666687] transition-colors hover:border-[#c0c0cf] hover:bg-[#f6f6f9] hover:text-[#32324d] cursor-pointer"
            >
              <RotateCcw className="h-3.5 w-3.5" />
              Show guided tour
            </button>
          )}
          <button
            type="button"
            onClick={() => setWidgetModalOpen(true)}
            className="inline-flex h-9 items-center gap-2 rounded-[4px] border border-[#dcdce4] bg-white px-4 text-xs font-semibold text-[#32324d] shadow-[0_1px_2px_rgba(33,33,52,0.04)] transition-colors hover:border-[#c0c0cf] hover:bg-[#f6f6f9] cursor-pointer"
          >
            <Plus className="h-3.5 w-3.5 text-[#32324d]" strokeWidth={2.3} />
            <span>Add Widget</span>
          </button>
        </div>
      </div>

      {overviewQuery.isError && (
        <div className="rounded-[4px] border border-amber-300 bg-amber-50 px-4 py-3 text-xs text-amber-950">
          <p className="font-semibold">Backend API unreachable.</p>
          <p className="mt-0.5 text-amber-900/80">
            Ensure the RAPI backend server is running so live entries and schema telemetry can load.
          </p>
        </div>
      )}

      {/* Guided Tour Split Card */}
      {!tourClosed && (
        <section className="grid grid-cols-1 overflow-hidden rounded-[4px] border border-[#eaeaef] bg-white shadow-[0_1px_4px_rgba(33,33,52,0.06)] lg:grid-cols-2">
          {/* Left column: Discover your application */}
          <div className="flex flex-col justify-between border-b border-[#eaeaef] p-7 lg:border-r lg:border-b-0 lg:p-8">
            <div>
              <WavingHandBadge />
              <h2 className="text-[18px] font-bold tracking-[-0.01em] text-[#32324d]">
                Discover your application!
              </h2>
              <p className="mt-1.5 text-[14px] text-[#666687]">
                Follow the guided tour to get the most out of RAPI.
              </p>

              <div className="mt-6">
                <div className="mb-2 text-xs font-medium text-[#666687]">
                  {progressPercent}% completed
                </div>
                <div className="h-2 w-full overflow-hidden rounded-full bg-[#eaeaef]">
                  <div
                    className="h-full rounded-full bg-[#4945ff] transition-all duration-300"
                    style={{ width: `${progressPercent}%` }}
                  />
                </div>
              </div>
            </div>

            <div className="mt-8">
              <button
                type="button"
                onClick={() => setTourClosed(true)}
                className="inline-flex h-9 items-center justify-center rounded-[4px] border border-[#dcdce4] bg-white px-4 text-xs font-semibold text-[#32324d] transition-colors hover:bg-[#f6f6f9] cursor-pointer"
              >
                Close guided tour
              </button>
            </div>
          </div>

          {/* Right column: Your tasks */}
          <div className="p-7 lg:p-8">
            <h3 className="mb-4 text-[13px] font-bold text-[#32324d]">Your tasks</h3>

            <div className="divide-y divide-[#eaeaef] overflow-hidden rounded-[4px] border border-[#eaeaef] bg-white">
              {tasks.map((task) => (
                <div
                  key={task.id}
                  className="flex items-center justify-between gap-3 px-4 py-3.5 transition-colors hover:bg-[#fafafd]"
                >
                  <div className="flex items-center gap-3 min-w-0">
                    {task.done ? (
                      <CheckCircle2
                        className="h-[18px] w-[18px] shrink-0 text-[#4945ff]"
                        strokeWidth={2.2}
                      />
                    ) : (
                      <span
                        className="h-[16px] w-[16px] shrink-0 rounded-full border-[1.5px] border-[#c0c0cf] bg-white"
                        aria-hidden="true"
                      />
                    )}
                    <span
                      className={cn(
                        "truncate text-[13px]",
                        task.done ? "text-[#666687]" : "text-[#32324d]",
                      )}
                    >
                      {task.label}
                    </span>
                  </div>

                  {task.external ? (
                    <a
                      href={task.to}
                      target="_blank"
                      rel="noreferrer"
                      onClick={markDeployRead}
                      className="inline-flex shrink-0 items-center gap-1.5 text-[13px] font-medium text-[#4945ff] hover:text-[#271fe0] hover:underline"
                    >
                      <span>{task.cta}</span>
                      <ExternalLink className="h-3.5 w-3.5" />
                    </a>
                  ) : (
                    <Link
                      to={task.to}
                      className="inline-flex shrink-0 items-center gap-1 text-[13px] font-medium text-[#4945ff] hover:text-[#271fe0] hover:underline"
                    >
                      <span>{task.cta}</span>
                      <ChevronRight className="h-3.5 w-3.5" />
                    </Link>
                  )}
                </div>
              ))}
            </div>
          </div>
        </section>
      )}

      {/* 2-Column Widget Grid */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Widget 1: LAST EDITED ENTRIES */}
        {activeWidgets.includes("last-edited") && (
          <section className="flex flex-col rounded-[4px] border border-[#eaeaef] bg-white p-6 shadow-[0_1px_4px_rgba(33,33,52,0.06)]">
            <div className="mb-4 flex items-center gap-2">
              <Pencil className="h-3.5 w-3.5 text-[#8e8ea9]" />
              <h2 className="text-[11px] font-bold tracking-[0.06em] text-[#666687] uppercase">
                Last edited entries
              </h2>
            </div>

            {recentEntriesQuery.isLoading ? (
              <div className="space-y-2 rounded-[4px] border border-[#eaeaef] p-4">
                {[1, 2, 3, 4].map((n) => (
                  <div key={n} className="h-9 animate-pulse rounded bg-[#f6f6f9]" />
                ))}
              </div>
            ) : lastEditedEntries.length === 0 ? (
              <div className="flex-1 rounded-[4px] border border-transparent">
                <EmptyEntriesIllustration message="No edited entries" />
              </div>
            ) : (
              <div className="divide-y divide-[#eaeaef] overflow-hidden rounded-[4px] border border-[#eaeaef]">
                {lastEditedEntries.map((entry) => (
                  <div
                    key={entry.key}
                    className="grid grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)_84px_minmax(0,1fr)_32px] items-center gap-3 px-4 py-3.5 text-[13px] transition-colors hover:bg-[#fafafd]"
                  >
                    <Link
                      to={entry.editTo}
                      className="truncate font-medium text-[#32324d] hover:text-[#4945ff]"
                    >
                      {entry.title}
                    </Link>

                    <span className="truncate text-[#666687]">{entry.schemaLabel}</span>

                    <div>
                      {entry.status === "none" ? (
                        <span className="text-[#8e8ea9]">-</span>
                      ) : entry.status === "published" ? (
                        <span className="inline-flex items-center rounded-[4px] border border-[#c6f0c2] bg-[#eafbe7] px-2 py-0.5 text-[11px] font-semibold text-[#2f6846]">
                          Published
                        </span>
                      ) : entry.status === "modified" ? (
                        <span className="inline-flex items-center rounded-[4px] border border-[#f5c0b8] bg-[#fdf4dc] px-2 py-0.5 text-[11px] font-semibold text-[#be5d01]">
                          Modified
                        </span>
                      ) : (
                        <span className="inline-flex items-center rounded-[4px] border border-[#bae0ff] bg-[#eaf5ff] px-2.5 py-0.5 text-[11px] font-semibold text-[#0c75af]">
                          Draft
                        </span>
                      )}
                    </div>

                    <span className="truncate text-[12px] text-[#666687]">
                      {formatRelativeTime(entry.updatedAt)}
                    </span>

                    <Link
                      to={entry.editTo}
                      title={`Edit ${entry.title}`}
                      aria-label={`Edit ${entry.title}`}
                      className="inline-flex h-7 w-7 items-center justify-center rounded text-[#666687] transition-colors hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Link>
                  </div>
                ))}
              </div>
            )}
          </section>
        )}

        {/* Widget 2: LAST PUBLISHED ENTRIES */}
        {activeWidgets.includes("last-published") && (
          <section className="flex flex-col rounded-[4px] border border-[#eaeaef] bg-white p-6 shadow-[0_1px_4px_rgba(33,33,52,0.06)]">
            <div className="mb-4 flex items-center gap-2">
              <CheckCircle2 className="h-3.5 w-3.5 text-[#8e8ea9]" />
              <h2 className="text-[11px] font-bold tracking-[0.06em] text-[#666687] uppercase">
                Last published entries
              </h2>
            </div>

            {recentEntriesQuery.isLoading ? (
              <div className="space-y-2 rounded-[4px] border border-[#eaeaef] p-4">
                {[1, 2, 3].map((n) => (
                  <div key={n} className="h-9 animate-pulse rounded bg-[#f6f6f9]" />
                ))}
              </div>
            ) : lastPublishedEntries.length === 0 ? (
              <div className="flex flex-1 items-center justify-center">
                <EmptyEntriesIllustration message="No published entries" />
              </div>
            ) : (
              <div className="divide-y divide-[#eaeaef] overflow-hidden rounded-[4px] border border-[#eaeaef]">
                {lastPublishedEntries.map((entry) => (
                  <div
                    key={`pub-${entry.key}`}
                    className="grid grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)_88px_minmax(0,1fr)_32px] items-center gap-3 px-4 py-3.5 text-[13px] transition-colors hover:bg-[#fafafd]"
                  >
                    <Link
                      to={entry.editTo}
                      className="truncate font-medium text-[#32324d] hover:text-[#4945ff]"
                    >
                      {entry.title}
                    </Link>

                    <span className="truncate text-[#666687]">{entry.schemaLabel}</span>

                    <div>
                      <span className="inline-flex items-center rounded-[4px] border border-[#c6f0c2] bg-[#eafbe7] px-2 py-0.5 text-[11px] font-semibold text-[#2f6846]">
                        Published
                      </span>
                    </div>

                    <span className="truncate text-[12px] text-[#666687]">
                      {formatRelativeTime(entry.publishedAt ?? entry.updatedAt)}
                    </span>

                    <Link
                      to={entry.editTo}
                      title={`Edit ${entry.title}`}
                      aria-label={`Edit ${entry.title}`}
                      className="inline-flex h-7 w-7 items-center justify-center rounded text-[#666687] transition-colors hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Link>
                  </div>
                ))}
              </div>
            )}
          </section>
        )}

        {/* Widget 3: PROFILE */}
        {activeWidgets.includes("profile") && (
          <section className="flex flex-col justify-between rounded-[4px] border border-[#eaeaef] bg-white p-6 shadow-[0_1px_4px_rgba(33,33,52,0.06)]">
            <div className="mb-4 flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <User className="h-3.5 w-3.5 text-[#8e8ea9]" />
                <h2 className="text-[11px] font-bold tracking-[0.06em] text-[#666687] uppercase">
                  Profile
                </h2>
              </div>
              <Link
                to="/account"
                className="text-[13px] font-medium text-[#4945ff] hover:text-[#271fe0] hover:underline"
              >
                Profile settings
              </Link>
            </div>

            <div className="divide-y divide-[#eaeaef] overflow-hidden rounded-[4px] border border-[#eaeaef]">
              <div className="flex items-center justify-between px-4 py-3 text-[13px]">
                <span className="text-[#666687]">Name</span>
                <span className="font-medium text-[#32324d]">{fullName}</span>
              </div>
              <div className="flex items-center justify-between px-4 py-3 text-[13px]">
                <span className="text-[#666687]">Email</span>
                <span className="font-mono text-xs text-[#32324d]">{user?.email ?? "—"}</span>
              </div>
              <div className="flex items-center justify-between px-4 py-3 text-[13px]">
                <span className="text-[#666687]">Roles</span>
                <div className="flex flex-wrap items-center gap-1.5">
                  {user?.isSuperAdmin ? (
                    <span className="inline-flex items-center rounded-[4px] bg-[#f0f0ff] px-2 py-0.5 text-[11px] font-semibold text-[#4945ff]">
                      Super Admin
                    </span>
                  ) : user?.roles && user.roles.length > 0 ? (
                    user.roles.map((r) => (
                      <span
                        key={r.id}
                        className="inline-flex items-center rounded-[4px] bg-[#f0f0ff] px-2 py-0.5 text-[11px] font-semibold text-[#4945ff]"
                      >
                        {r.name}
                      </span>
                    ))
                  ) : (
                    <span className="inline-flex items-center rounded-[4px] bg-[#f6f6f9] px-2 py-0.5 text-[11px] font-medium text-[#666687]">
                      Editor
                    </span>
                  )}
                </div>
              </div>
            </div>
          </section>
        )}

        {/* Widget 4: ENTRIES */}
        {activeWidgets.includes("entries") && (
          <section className="flex flex-col justify-between rounded-[4px] border border-[#eaeaef] bg-white p-6 shadow-[0_1px_4px_rgba(33,33,52,0.06)]">
            <div className="mb-4 flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Layers className="h-3.5 w-3.5 text-[#8e8ea9]" />
                <h2 className="text-[11px] font-bold tracking-[0.06em] text-[#666687] uppercase">
                  Entries
                </h2>
              </div>
              <Link
                to="/content-manager"
                className="text-[13px] font-medium text-[#4945ff] hover:text-[#271fe0] hover:underline"
              >
                Open Content Manager
              </Link>
            </div>

            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <Link
                to="/content-manager"
                className="rounded-[4px] border border-[#eaeaef] bg-[#fafafd] p-3.5 transition-colors hover:border-[#d9d8ff] hover:bg-[#f0f0ff]/40"
              >
                <div className="tabular text-xl font-bold text-[#32324d]">
                  {stats?.documents ?? 0}
                </div>
                <div className="mt-1 text-xs text-[#666687]">Documents</div>
              </Link>

              <Link
                to="/content-type-builder"
                className="rounded-[4px] border border-[#eaeaef] bg-[#fafafd] p-3.5 transition-colors hover:border-[#d9d8ff] hover:bg-[#f0f0ff]/40"
              >
                <div className="tabular text-xl font-bold text-[#32324d]">
                  {stats?.contentTypes ?? contentTypes.length}
                </div>
                <div className="mt-1 text-xs text-[#666687]">Content-Types</div>
              </Link>

              <Link
                to="/media-library"
                className="rounded-[4px] border border-[#eaeaef] bg-[#fafafd] p-3.5 transition-colors hover:border-[#d9d8ff] hover:bg-[#f0f0ff]/40"
              >
                <div className="tabular text-xl font-bold text-[#32324d]">
                  {stats?.mediaFiles ?? 0}
                </div>
                <div className="mt-1 text-xs text-[#666687]">Media assets</div>
              </Link>

              <Link
                to="/settings/i18n"
                className="rounded-[4px] border border-[#eaeaef] bg-[#fafafd] p-3.5 transition-colors hover:border-[#d9d8ff] hover:bg-[#f0f0ff]/40"
              >
                <div className="tabular text-xl font-bold text-[#32324d]">
                  {stats?.locales ?? 1}
                </div>
                <div className="mt-1 text-xs text-[#666687]">Locales</div>
              </Link>
            </div>
          </section>
        )}

        {/* Optional Widget 5: RUNTIME TELEMETRY */}
        {activeWidgets.includes("runtime") && (
          <section className="flex flex-col justify-between rounded-[4px] border border-[#eaeaef] bg-white p-6 shadow-[0_1px_4px_rgba(33,33,52,0.06)] lg:col-span-2">
            <div className="mb-4 flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Server className="h-3.5 w-3.5 text-[#8e8ea9]" />
                <h2 className="text-[11px] font-bold tracking-[0.06em] text-[#666687] uppercase">
                  Runtime telemetry
                </h2>
              </div>
              <a
                href="/openapi.json"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-xs font-medium text-[#4945ff] hover:underline"
              >
                /openapi.json
                <ExternalLink className="h-3 w-3" />
              </a>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-4 text-xs">
              <div className="rounded-[4px] border border-[#eaeaef] px-4 py-3">
                <div className="text-[#666687]">Database</div>
                <div className="mt-1 font-semibold text-[#32324d]">
                  SQLite ({overviewQuery.data?.sqliteJournalMode ?? "WAL"})
                </div>
              </div>
              <div className="rounded-[4px] border border-[#eaeaef] px-4 py-3">
                <div className="text-[#666687]">Cache store</div>
                <div className="mt-1 font-semibold text-[#32324d]">
                  {overviewQuery.data?.cacheBackend === "redis" ? "Redis" : "Moka In-Memory"}
                </div>
              </div>
              <div className="rounded-[4px] border border-[#eaeaef] px-4 py-3">
                <div className="text-[#666687]">Queue worker</div>
                <div className="mt-1 font-semibold text-[#32324d]">
                  {stats?.pendingJobs ?? 0} pending ({overviewQuery.data?.queueBackend ?? "local"})
                </div>
              </div>
              <div className="rounded-[4px] border border-[#eaeaef] px-4 py-3">
                <div className="text-[#666687]">Process uptime</div>
                <div className="mt-1 font-mono font-semibold text-[#32324d]">
                  {overviewQuery.data ? formatUptime(overviewQuery.data.uptimeSeconds) : "—"}
                </div>
              </div>
            </div>
          </section>
        )}
      </div>

      {/* Add / Customize Widgets Modal */}
      <Dialog open={widgetModalOpen} onOpenChange={setWidgetModalOpen}>
        <DialogContent
          title="Customize Home Widgets"
          description="Choose which widgets appear on your administration home page."
          className="max-w-lg"
        >
          <div className="divide-y divide-[#eaeaef] rounded-[4px] border border-[#eaeaef]">
            {WIDGET_DEFINITIONS.map((widget) => {
              const checked = activeWidgets.includes(widget.id);
              return (
                <label
                  key={widget.id}
                  htmlFor={`widget-toggle-${widget.id}`}
                  className="flex cursor-pointer items-start gap-3 px-4 py-3 transition-colors hover:bg-[#fafafd]"
                >
                  <div className="pt-0.5">
                    <Checkbox
                      id={`widget-toggle-${widget.id}`}
                      checked={checked}
                      onCheckedChange={() => toggleWidget(widget.id)}
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-xs font-semibold text-[#32324d]">{widget.title}</div>
                    <p className="mt-0.5 text-[11px] leading-relaxed text-[#666687]">
                      {widget.description}
                    </p>
                  </div>
                </label>
              );
            })}
          </div>

          <div className="mt-4 flex items-center justify-between">
            <button
              type="button"
              onClick={() =>
                setActiveWidgets(
                  WIDGET_DEFINITIONS.filter((w) => w.defaultVisible).map((w) => w.id),
                )
              }
              className="text-xs font-medium text-[#666687] hover:text-[#32324d] cursor-pointer"
            >
              Reset to default
            </button>
            <button
              type="button"
              onClick={() => setWidgetModalOpen(false)}
              className="inline-flex h-8 items-center justify-center rounded-[4px] bg-[#4945ff] px-4 text-xs font-semibold text-white hover:bg-[#271fe0] cursor-pointer"
            >
              Done
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
