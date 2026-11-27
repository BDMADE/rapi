import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle2,
  Database,
  FilePenLine,
  Plus,
  Search,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useUiStore } from "@/app/store";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  DataTable,
  EmptyState,
  Input,
  Select,
  useToast,
} from "@/components/ui";
import { api, type ContentDocument } from "@/lib/api";

export function ContentManagerPage() {
  const { uid } = useParams();
  const navigate = useNavigate();
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const activeLocale = useUiStore((s) => s.activeLocale);

  const catalogQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });

  const contentTypes = catalogQuery.data?.contentTypes ?? [];
  const collectionTypes = contentTypes.filter((s) => s.kind === "collectionType");
  const singleTypes = contentTypes.filter((s) => s.kind === "singleType");

  useEffect(() => {
    if (!uid && contentTypes.length > 0) {
      const first = contentTypes[0];
      if (first.kind === "singleType") {
        navigate(`/content-manager/${encodeURIComponent(first.uid)}/single`, { replace: true });
      } else {
        navigate(`/content-manager/${encodeURIComponent(first.uid)}`, { replace: true });
      }
    }
  }, [uid, contentTypes, navigate]);

  const activeSchema = contentTypes.find((s) => s.uid === uid);

  useEffect(() => {
    if (activeSchema?.kind === "singleType") {
      navigate(`/content-manager/${encodeURIComponent(activeSchema.uid)}/single`, {
        replace: true,
      });
    }
  }, [activeSchema, navigate]);

  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "draft" | "published">("all");
  const [sort, setSort] = useState("updatedAt:desc");
  const [page, setPage] = useState(1);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const entriesQuery = useQuery({
    queryKey: [
      "cm-entries",
      activeSchema?.uid,
      activeLocale,
      search,
      statusFilter,
      sort,
      page,
    ],
    queryFn: () =>
      api.listEntries(activeSchema!.uid, {
        page,
        pageSize: 20,
        locale: activeLocale,
        status: statusFilter === "all" ? undefined : statusFilter,
        q: search.trim() || undefined,
        sort,
      }),
    enabled: Boolean(activeSchema && activeSchema.kind === "collectionType"),
  });

  const bulkPublishMut = useMutation({
    mutationFn: () =>
      api.bulkPublishEntries(activeSchema!.uid, Array.from(selectedIds), activeLocale),
    onSuccess: (res) => {
      setSelectedIds(new Set());
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: `Published ${res.data.publishedCount} entries`, tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Bulk publish failed", description: e.message, tone: "error" }),
  });

  const bulkDeleteMut = useMutation({
    mutationFn: () => api.bulkDeleteEntries(activeSchema!.uid, Array.from(selectedIds)),
    onSuccess: (res) => {
      setSelectedIds(new Set());
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: `Deleted ${res.data.deletedCount} entries`, tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Bulk delete failed", description: e.message, tone: "error" }),
  });

  const rows = entriesQuery.data?.data ?? [];
  const pagination = entriesQuery.data?.meta.pagination;

  const previewColumns = (activeSchema?.fields ?? [])
    .filter(
      (f) =>
        f.fieldType === "string" ||
        f.fieldType === "uid" ||
        f.fieldType === "enumeration" ||
        f.fieldType === "boolean" ||
        f.fieldType === "integer",
    )
    .slice(0, 3);

  return (
    <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
      {/* Left Content Types Rail */}
      <Card className="lg:col-span-3">
        <div className="divide-y divide-zinc-200">
          <div className="p-3">
            <div className="mb-1.5 text-[11px] font-semibold text-zinc-500">
              Collection Types ({collectionTypes.length})
            </div>
            <div className="space-y-0.5">
              {collectionTypes.map((ct) => (
                <Link
                  key={ct.uid}
                  to={`/content-manager/${encodeURIComponent(ct.uid)}`}
                  className={`flex items-center justify-between rounded px-2 py-1.5 text-xs ${
                    ct.uid === activeSchema?.uid
                      ? "bg-indigo-50 font-medium text-indigo-900"
                      : "text-zinc-700 hover:bg-zinc-50"
                  }`}
                >
                  <span className="truncate">{ct.displayName}</span>
                  <span className="font-mono text-[10px] text-zinc-400">/{ct.pluralName}</span>
                </Link>
              ))}
            </div>
          </div>

          <div className="p-3">
            <div className="mb-1.5 text-[11px] font-semibold text-zinc-500">
              Single Types ({singleTypes.length})
            </div>
            <div className="space-y-0.5">
              {singleTypes.map((st) => (
                <Link
                  key={st.uid}
                  to={`/content-manager/${encodeURIComponent(st.uid)}/single`}
                  className={`flex items-center justify-between rounded px-2 py-1.5 text-xs ${
                    st.uid === activeSchema?.uid
                      ? "bg-indigo-50 font-medium text-indigo-900"
                      : "text-zinc-700 hover:bg-zinc-50"
                  }`}
                >
                  <span className="truncate">{st.displayName}</span>
                  <span className="font-mono text-[10px] text-zinc-400">/{st.singularName}</span>
                </Link>
              ))}
            </div>
          </div>
        </div>
      </Card>

      {/* Main Entries Workspace */}
      <div className="space-y-4 lg:col-span-9">
        {!activeSchema ? (
          <EmptyState
            icon={Database}
            title="No content types available"
            description="Define a collection type or single type in Content-Type Builder to begin authoring documents."
            action={
              <Button size="sm" asChild>
                <Link to="/content-type-builder">Open Content-Type Builder</Link>
              </Button>
            }
          />
        ) : (
          <>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
                    {activeSchema.displayName}
                  </h1>
                  <Badge tone="mono">/api/{activeSchema.pluralName}</Badge>
                  <Badge tone="neutral">locale: {activeLocale}</Badge>
                </div>
                <p className="mt-0.5 text-xs text-zinc-600">
                  {pagination?.total ?? 0} logical document{pagination?.total === 1 ? "" : "s"}
                </p>
              </div>

              <Button size="sm" asChild>
                <Link to={`/content-manager/${encodeURIComponent(activeSchema.uid)}/create`}>
                  <Plus className="h-3.5 w-3.5" />
                  Create entry
                </Link>
              </Button>
            </div>

            {/* Filter & Bulk Action Toolbar */}
            <div className="flex flex-wrap items-center justify-between gap-2.5 rounded-md border border-zinc-200 bg-white p-3">
              <div className="flex flex-1 flex-wrap items-center gap-2">
                <div className="relative min-w-52 flex-1">
                  <Search className="pointer-events-none absolute left-2.5 top-2 h-3.5 w-3.5 text-zinc-400" />
                  <Input
                    value={search}
                    onChange={(e) => {
                      setSearch(e.target.value);
                      setPage(1);
                    }}
                    placeholder="Full-text search (FTS5)..."
                    className="pl-8"
                  />
                </div>
                <div className="w-36">
                  <Select
                    value={statusFilter}
                    onValueChange={(v) => {
                      setStatusFilter(v as "all" | "draft" | "published");
                      setPage(1);
                    }}
                    options={[
                      { value: "all", label: "All statuses" },
                      { value: "draft", label: "Draft rows" },
                      { value: "published", label: "Published live" },
                    ]}
                  />
                </div>
                <div className="w-44">
                  <Select
                    value={sort}
                    onValueChange={setSort}
                    options={[
                      { value: "updatedAt:desc", label: "Updated (newest)" },
                      { value: "createdAt:desc", label: "Created (newest)" },
                      { value: "createdAt:asc", label: "Created (oldest)" },
                    ]}
                  />
                </div>
              </div>

              {selectedIds.size > 0 && (
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium text-zinc-600">
                    {selectedIds.size} selected
                  </span>
                  {activeSchema.draftAndPublish && (
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => bulkPublishMut.mutate()}
                      disabled={bulkPublishMut.isPending}
                    >
                      <CheckCircle2 className="h-3.5 w-3.5 text-emerald-600" />
                      Publish
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="danger"
                    onClick={() => bulkDeleteMut.mutate()}
                    disabled={bulkDeleteMut.isPending}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    Delete
                  </Button>
                </div>
              )}
            </div>

            <DataTable<ContentDocument>
              rows={rows}
              getRowKey={(r) => r.documentId}
              emptyMessage="No documents match your filters. Create an entry to get started."
              columns={[
                {
                  key: "select",
                  header: "",
                  className: "w-8",
                  render: (r) => (
                    <Checkbox
                      checked={selectedIds.has(r.documentId)}
                      onCheckedChange={(checked) => {
                        const next = new Set(selectedIds);
                        if (checked) next.add(r.documentId);
                        else next.delete(r.documentId);
                        setSelectedIds(next);
                      }}
                    />
                  ),
                },
                {
                  key: "documentId",
                  header: "Document ID",
                  render: (r) => (
                    <Link
                      to={`/content-manager/${encodeURIComponent(activeSchema.uid)}/${encodeURIComponent(r.documentId)}`}
                      className="font-mono text-[11px] font-medium text-indigo-700 hover:underline"
                    >
                      {r.documentId.slice(0, 10)}…
                    </Link>
                  ),
                },
                ...previewColumns.map((f) => ({
                  key: f.name,
                  header: f.name,
                  render: (r: ContentDocument) => {
                    const val = r[f.name];
                    if (typeof val === "boolean") {
                      return <Badge tone={val ? "emerald" : "neutral"}>{String(val)}</Badge>;
                    }
                    return (
                      <span className="max-w-52 truncate block text-zinc-800">
                        {val === null || val === undefined ? "—" : String(val)}
                      </span>
                    );
                  },
                })),
                {
                  key: "state",
                  header: "State",
                  render: (r) => {
                    const st =
                      r.publicationState ?? (r.publishedAt ? "published" : "draft");
                    return (
                      <Badge
                        tone={
                          st === "published"
                            ? "emerald"
                            : st === "modified"
                              ? "amber"
                              : "neutral"
                        }
                      >
                        {st}
                      </Badge>
                    );
                  },
                },
                {
                  key: "updatedAt",
                  header: "Updated",
                  render: (r) => (
                    <span className="tabular font-mono text-[11px] text-zinc-500">
                      {new Date(r.updatedAt).toLocaleString()}
                    </span>
                  ),
                },
                {
                  key: "edit",
                  header: "",
                  className: "text-right",
                  render: (r) => (
                    <Button size="sm" variant="ghost" asChild>
                      <Link
                        to={`/content-manager/${encodeURIComponent(activeSchema.uid)}/${encodeURIComponent(r.documentId)}`}
                      >
                        <FilePenLine className="h-3.5 w-3.5" />
                      </Link>
                    </Button>
                  ),
                },
              ]}
            />

            {pagination && pagination.pageCount > 1 && (
              <div className="flex items-center justify-between text-xs text-zinc-600">
                <span>
                  Page {pagination.page} of {pagination.pageCount} ({pagination.total} total)
                </span>
                <div className="flex gap-1.5">
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={page <= 1}
                    onClick={() => setPage((p) => p - 1)}
                  >
                    Previous
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={page >= pagination.pageCount}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    Next
                  </Button>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
