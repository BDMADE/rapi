import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Copy,
  Database,
  Globe,
  ListFilter,
  MoreHorizontal,
  Pencil,
  Plus,
  Search,
  Settings,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useUiStore } from "@/app/store";
import { Checkbox } from "@/components/ui/checkbox";
import { useToast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";
import { api, type ContentDocument, type FieldDefinition } from "@/lib/api";
import { cn } from "@/lib/utils";

const DISPLAYABLE_FIELD_TYPES = new Set([
  "string",
  "text",
  "uid",
  "email",
  "enumeration",
  "boolean",
  "integer",
  "biginteger",
  "float",
  "decimal",
  "date",
  "datetime",
]);

function getDefaultVisibleColumns(fields: FieldDefinition[]): string[] {
  const candidates = fields.filter((f) => DISPLAYABLE_FIELD_TYPES.has(f.fieldType));
  if (candidates.length === 0) return [];
  return candidates.slice(0, 4).map((f) => f.name);
}

function formatCellText(val: unknown): string {
  if (val === null || val === undefined || val === "") return "-";
  if (typeof val === "boolean") return val ? "true" : "false";
  if (typeof val === "number") return String(val);
  if (typeof val === "string") return val;
  return JSON.stringify(val);
}

export function ContentManagerPage() {
  const { uid } = useParams();
  const navigate = useNavigate();
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const activeLocale = useUiStore((s) => s.activeLocale);
  const setActiveLocale = useUiStore((s) => s.setActiveLocale);

  const catalogQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });

  const localesQuery = useQuery({
    queryKey: ["locales"],
    queryFn: api.listLocales,
  });

  const contentTypes = catalogQuery.data?.contentTypes ?? [];
  const configuredLocales = localesQuery.data ?? [
    { id: 1, code: "en", name: "English (en)", isDefault: true },
  ];

  useEffect(() => {
    if (!uid && contentTypes.length > 0) {
      const firstCollection = contentTypes.find((s) => s.kind === "collectionType");
      const target = firstCollection ?? contentTypes[0];
      if (target.kind === "singleType") {
        navigate(`/content-manager/${encodeURIComponent(target.uid)}/single`, { replace: true });
      } else {
        navigate(`/content-manager/${encodeURIComponent(target.uid)}`, { replace: true });
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

  // Search, Filters, Sort & Pagination State
  const [searchOpen, setSearchOpen] = useState(false);
  const [search, setSearch] = useState("");
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  const [statusFilter, setStatusFilter] = useState<"all" | "draft" | "published">("all");
  const [sortField, setSortField] = useState<string>("id");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Per-schema customizable visible columns persisted in localStorage
  const [visibleFieldNames, setVisibleFieldNames] = useState<string[]>([]);

  useEffect(() => {
    if (!activeSchema) return;
    setSelectedIds(new Set());
    setPage(1);
    const storageKey = `rapi_cm_cols_${activeSchema.uid}`;
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) {
        const parsed = JSON.parse(saved) as string[];
        const validSet = new Set(activeSchema.fields.map((f) => f.name));
        const filtered = parsed.filter((name) => validSet.has(name));
        if (filtered.length > 0) {
          setVisibleFieldNames(filtered);
          return;
        }
      }
    } catch {
      // ignore storage errors
    }
    setVisibleFieldNames(getDefaultVisibleColumns(activeSchema.fields));
  }, [activeSchema]);

  const toggleColumnVisibility = (fieldName: string) => {
    if (!activeSchema) return;
    setVisibleFieldNames((prev) => {
      const next = prev.includes(fieldName)
        ? prev.filter((f) => f !== fieldName)
        : [...prev, fieldName];
      try {
        localStorage.setItem(`rapi_cm_cols_${activeSchema.uid}`, JSON.stringify(next));
      } catch {
        // ignore
      }
      return next;
    });
  };

  const resetColumns = () => {
    if (!activeSchema) return;
    const defaults = getDefaultVisibleColumns(activeSchema.fields);
    setVisibleFieldNames(defaults);
    try {
      localStorage.removeItem(`rapi_cm_cols_${activeSchema.uid}`);
    } catch {
      // ignore
    }
  };

  const sortParam = `${sortField}:${sortDir}`;

  const entriesQuery = useQuery({
    queryKey: [
      "cm-entries",
      activeSchema?.uid,
      activeLocale,
      search,
      statusFilter,
      sortParam,
      page,
      pageSize,
    ],
    queryFn: () =>
      api.listEntries(activeSchema!.uid, {
        page,
        pageSize,
        locale: activeLocale,
        status: statusFilter === "all" ? undefined : statusFilter,
        q: search.trim() || undefined,
        sort: sortParam,
      }),
    enabled: Boolean(activeSchema && activeSchema.kind === "collectionType"),
  });

  const rows = entriesQuery.data?.data ?? [];
  const pagination = entriesQuery.data?.meta.pagination;
  const totalCount = pagination?.total ?? rows.length;

  const displayableFields = useMemo(
    () => (activeSchema?.fields ?? []).filter((f) => DISPLAYABLE_FIELD_TYPES.has(f.fieldType)),
    [activeSchema],
  );

  const activeColumns = useMemo(() => {
    const map = new Map(displayableFields.map((f) => [f.name, f]));
    return visibleFieldNames
      .map((name) => map.get(name))
      .filter((f): f is FieldDefinition => Boolean(f));
  }, [displayableFields, visibleFieldNames]);

  // Mutations for row & bulk actions
  const invalidateList = () => {
    queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
    queryClient.invalidateQueries({ queryKey: ["home-recent-entries"] });
    queryClient.invalidateQueries({ queryKey: ["system-overview"] });
  };

  const bulkPublishMut = useMutation({
    mutationFn: () =>
      api.bulkPublishEntries(activeSchema!.uid, Array.from(selectedIds), activeLocale),
    onSuccess: (res) => {
      setSelectedIds(new Set());
      invalidateList();
      pushToast({ title: `Published ${res.data.publishedCount} entries`, tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Bulk publish failed", description: e.message, tone: "error" }),
  });

  const bulkDeleteMut = useMutation({
    mutationFn: () => api.bulkDeleteEntries(activeSchema!.uid, Array.from(selectedIds)),
    onSuccess: (res) => {
      setSelectedIds(new Set());
      invalidateList();
      pushToast({ title: `Deleted ${res.data.deletedCount} entries`, tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Bulk delete failed", description: e.message, tone: "error" }),
  });

  const rowPublishMut = useMutation({
    mutationFn: (documentId: string) =>
      api.publishEntry(activeSchema!.uid, documentId, activeLocale),
    onSuccess: () => {
      invalidateList();
      pushToast({ title: "Entry published", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Publish failed", description: e.message, tone: "error" }),
  });

  const rowUnpublishMut = useMutation({
    mutationFn: (documentId: string) =>
      api.unpublishEntry(activeSchema!.uid, documentId, activeLocale),
    onSuccess: () => {
      invalidateList();
      pushToast({ title: "Entry unpublished", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Unpublish failed", description: e.message, tone: "error" }),
  });

  const rowDeleteMut = useMutation({
    mutationFn: (documentId: string) => api.deleteEntry(activeSchema!.uid, documentId),
    onSuccess: () => {
      invalidateList();
      pushToast({ title: "Entry deleted", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Delete failed", description: e.message, tone: "error" }),
  });

  const rowDuplicateMut = useMutation({
    mutationFn: async (doc: ContentDocument) => {
      if (!activeSchema) throw new Error("Missing schema");
      const payload: Record<string, unknown> = {};
      for (const f of activeSchema.fields) {
        if (f.name in doc && doc[f.name] !== undefined) {
          if (f.fieldType === "uid" || f.unique) {
            if (typeof doc[f.name] === "string") {
              payload[f.name] = `${doc[f.name]}-copy-${Math.floor(100 + Math.random() * 900)}`;
            }
          } else {
            payload[f.name] = doc[f.name];
          }
        }
      }
      return api.createEntry(activeSchema.uid, payload, activeLocale);
    },
    onSuccess: (created) => {
      invalidateList();
      pushToast({ title: "Entry duplicated", tone: "success" });
      if (activeSchema && created.documentId) {
        navigate(
          `/content-manager/${encodeURIComponent(activeSchema.uid)}/${encodeURIComponent(created.documentId)}`,
        );
      }
    },
    onError: (e: Error) =>
      pushToast({ title: "Duplicate failed", description: e.message, tone: "error" }),
  });

  const handleSortColumn = (colName: string) => {
    if (sortField === colName) {
      setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
    } else {
      setSortField(colName);
      setSortDir("asc");
    }
    setPage(1);
  };

  const allSelected = rows.length > 0 && rows.every((r) => selectedIds.has(r.documentId));

  const toggleSelectAll = (checked: boolean) => {
    if (checked) {
      setSelectedIds(new Set(rows.map((r) => r.documentId)));
    } else {
      setSelectedIds(new Set());
    }
  };

  if (catalogQuery.isLoading) {
    return (
      <div className="space-y-4 py-4">
        <div className="h-8 w-44 animate-pulse rounded bg-[#eaeaef]" />
        <div className="h-64 animate-pulse rounded-[4px] border border-[#eaeaef] bg-white" />
      </div>
    );
  }

  if (!activeSchema) {
    return (
      <div className="flex flex-col items-center justify-center rounded-[4px] border border-[#eaeaef] bg-white px-6 py-16 text-center shadow-[0_1px_4px_rgba(33,33,52,0.06)]">
        <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-[#f0f0ff] text-[#4945ff]">
          <Database className="h-6 w-6" />
        </div>
        <h1 className="text-lg font-bold text-[#32324d]">No content types yet</h1>
        <p className="mt-1 max-w-md text-[13px] text-[#666687]">
          Create your first Collection Type or Single Type in the Content-Type Builder to begin managing entries.
        </p>
        <Link
          to="/content-type-builder"
          className="mt-5 inline-flex h-9 items-center gap-2 rounded-[4px] bg-[#4945ff] px-4 text-xs font-semibold text-white transition-colors hover:bg-[#271fe0]"
        >
          <Plus className="h-4 w-4" />
          Open Content-Type Builder
        </Link>
      </div>
    );
  }

  return (
    <div>
      {/* Back Link */}
      <button
        type="button"
        onClick={() => navigate(-1)}
        className="mb-2.5 inline-flex items-center gap-1.5 text-[13px] font-medium text-[#4945ff] transition-colors hover:text-[#271fe0] cursor-pointer"
      >
        <ArrowLeft className="h-3.5 w-3.5" strokeWidth={2.2} />
        <span>Back</span>
      </button>

      {/* Title & Primary Action Row */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-[28px] leading-[1.22] font-bold tracking-[-0.02em] text-[#32324d]">
            {activeSchema.displayName}
          </h1>
          <p className="mt-1 text-[14px] text-[#666687]">
            {totalCount} {totalCount === 1 ? "entry" : "entries"} found
          </p>
        </div>

        <div className="flex items-center gap-2.5 pt-1">
          {/* Locale Selector Button (Globe Icon Square matching Strapi screenshot) */}
          <DropdownMenu.Root>
            <Tooltip content={`Locale: ${activeLocale}`}>
              <DropdownMenu.Trigger asChild>
                <button
                  type="button"
                  aria-label="Switch locale"
                  className="inline-flex h-9 min-w-[36px] items-center justify-center gap-1.5 rounded-[4px] border border-[#dcdce4] bg-white px-2.5 text-xs font-medium text-[#32324d] shadow-[0_1px_2px_rgba(33,33,52,0.04)] transition-colors hover:border-[#c0c0cf] hover:bg-[#f6f6f9] cursor-pointer"
                >
                  <Globe className="h-4 w-4 text-[#666687]" />
                  {configuredLocales.length > 1 && (
                    <span className="text-xs font-semibold text-[#32324d]">{activeLocale}</span>
                  )}
                </button>
              </DropdownMenu.Trigger>
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="end"
                sideOffset={6}
                className="z-50 min-w-44 rounded-[4px] border border-[#eaeaef] bg-white p-1 shadow-lg"
              >
                {configuredLocales.map((loc) => (
                  <DropdownMenu.Item
                    key={loc.code}
                    onSelect={() => {
                      setActiveLocale(loc.code);
                      setPage(1);
                    }}
                    className={cn(
                      "flex cursor-pointer items-center justify-between gap-3 rounded-[4px] px-2.5 py-1.5 text-xs outline-none",
                      loc.code === activeLocale
                        ? "bg-[#f0f0ff] font-semibold text-[#4945ff]"
                        : "text-[#32324d] hover:bg-[#f6f6f9]",
                    )}
                  >
                    <span className="truncate">{loc.name}</span>
                    <span className="font-mono text-[11px]">{loc.code}</span>
                  </DropdownMenu.Item>
                ))}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>

          {/* Primary Create Entry CTA */}
          <Link
            to={`/content-manager/${encodeURIComponent(activeSchema.uid)}/create`}
            className="inline-flex h-9 items-center gap-2 rounded-[4px] bg-[#4945ff] px-4 text-xs font-semibold text-white shadow-[0_1px_2px_rgba(33,33,52,0.08)] transition-colors hover:bg-[#271fe0]"
          >
            <Plus className="h-4 w-4" strokeWidth={2.4} />
            <span>Create new entry</span>
          </Link>
        </div>
      </div>

      {/* Toolbar Row: Search Icon/Input + Filters + Bulk Actions (left) | Gear Column Customizer (right) */}
      <div className="mt-7 mb-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          {/* Expandable Search Button / Input */}
          {searchOpen || search ? (
            <div className="relative flex items-center">
              <Search className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-[#8e8ea9]" />
              <input
                ref={searchInputRef}
                type="text"
                value={search}
                onChange={(e) => {
                  setSearch(e.target.value);
                  setPage(1);
                }}
                onBlur={() => {
                  if (!search.trim()) setSearchOpen(false);
                }}
                placeholder={`Search ${activeSchema.displayName.toLowerCase()}...`}
                className="h-8 w-56 rounded-[4px] border border-[#4945ff] bg-white pr-7 pl-8 text-xs text-[#32324d] placeholder:text-[#8e8ea9] focus:outline-none"
              />
              <button
                type="button"
                onClick={() => {
                  setSearch("");
                  setSearchOpen(false);
                  setPage(1);
                }}
                aria-label="Close search"
                className="absolute right-2 text-[#8e8ea9] hover:text-[#32324d] cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          ) : (
            <Tooltip content="Search entries">
              <button
                type="button"
                onClick={() => {
                  setSearchOpen(true);
                  setTimeout(() => searchInputRef.current?.focus(), 20);
                }}
                aria-label="Search entries"
                className="inline-flex h-8 w-8 items-center justify-center rounded-[4px] border border-[#dcdce4] bg-white text-[#32324d] transition-colors hover:border-[#c0c0cf] hover:bg-[#f6f6f9] cursor-pointer"
              >
                <Search className="h-3.5 w-3.5 text-[#666687]" strokeWidth={2} />
              </button>
            </Tooltip>
          )}

          {/* Filters Dropdown */}
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                className={cn(
                  "inline-flex h-8 items-center gap-2 rounded-[4px] border bg-white px-3 text-xs font-semibold transition-colors cursor-pointer",
                  statusFilter !== "all"
                    ? "border-[#4945ff] bg-[#f0f0ff] text-[#4945ff]"
                    : "border-[#dcdce4] text-[#32324d] hover:border-[#c0c0cf] hover:bg-[#f6f6f9]",
                )}
              >
                <ListFilter className="h-3.5 w-3.5" strokeWidth={2} />
                <span>Filters</span>
                {statusFilter !== "all" && (
                  <span className="ml-0.5 rounded-full bg-[#4945ff] px-1.5 py-0.2 text-[10px] text-white">
                    1
                  </span>
                )}
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="start"
                sideOffset={6}
                className="z-50 w-56 rounded-[4px] border border-[#eaeaef] bg-white p-2 shadow-lg"
              >
                <div className="px-2 pb-1.5 text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase">
                  Publication status
                </div>
                {[
                  { value: "all", label: "All entries" },
                  { value: "draft", label: "Draft only" },
                  { value: "published", label: "Published only" },
                ].map((opt) => (
                  <DropdownMenu.Item
                    key={opt.value}
                    onSelect={() => {
                      setStatusFilter(opt.value as "all" | "draft" | "published");
                      setPage(1);
                    }}
                    className={cn(
                      "flex cursor-pointer items-center justify-between rounded-[4px] px-2.5 py-1.5 text-xs outline-none",
                      statusFilter === opt.value
                        ? "bg-[#f0f0ff] font-semibold text-[#4945ff]"
                        : "text-[#32324d] hover:bg-[#f6f6f9]",
                    )}
                  >
                    <span>{opt.label}</span>
                  </DropdownMenu.Item>
                ))}

                <div className="my-1.5 h-px bg-[#eaeaef]" />
                <div className="px-2 pb-1.5 text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase">
                  Sort order
                </div>
                {[
                  { field: "id", dir: "asc" as const, label: "ID (Ascending)" },
                  { field: "id", dir: "desc" as const, label: "ID (Descending)" },
                  { field: "updatedAt", dir: "desc" as const, label: "Recently updated" },
                  { field: "createdAt", dir: "desc" as const, label: "Newest created" },
                ].map((s) => {
                  const active = sortField === s.field && sortDir === s.dir;
                  return (
                    <DropdownMenu.Item
                      key={`${s.field}:${s.dir}`}
                      onSelect={() => {
                        setSortField(s.field);
                        setSortDir(s.dir);
                        setPage(1);
                      }}
                      className={cn(
                        "flex cursor-pointer items-center justify-between rounded-[4px] px-2.5 py-1.5 text-xs outline-none",
                        active
                          ? "bg-[#f0f0ff] font-semibold text-[#4945ff]"
                          : "text-[#32324d] hover:bg-[#f6f6f9]",
                      )}
                    >
                      <span>{s.label}</span>
                    </DropdownMenu.Item>
                  );
                })}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>

          {/* Active filter pill */}
          {statusFilter !== "all" && (
            <span className="inline-flex h-8 items-center gap-1.5 rounded-[4px] border border-[#d9d8ff] bg-[#f0f0ff] px-2.5 text-xs font-medium text-[#4945ff]">
              <span>status is {statusFilter}</span>
              <button
                type="button"
                onClick={() => {
                  setStatusFilter("all");
                  setPage(1);
                }}
                aria-label="Clear status filter"
                className="rounded hover:text-[#271fe0] cursor-pointer"
              >
                <X className="h-3 w-3" />
              </button>
            </span>
          )}

          {/* Contextual Bulk Selection Bar */}
          {selectedIds.size > 0 && (
            <div className="ml-1 flex items-center gap-2 border-l border-[#dcdce4] pl-3">
              <span className="text-xs font-medium text-[#666687]">
                {selectedIds.size} {selectedIds.size === 1 ? "entry" : "entries"} selected
              </span>
              {activeSchema.draftAndPublish && (
                <button
                  type="button"
                  onClick={() => bulkPublishMut.mutate()}
                  disabled={bulkPublishMut.isPending}
                  className="inline-flex h-8 items-center gap-1.5 rounded-[4px] border border-[#c6f0c2] bg-[#eafbe7] px-3 text-xs font-semibold text-[#2f6846] transition-colors hover:bg-[#d6f6d0] disabled:opacity-50 cursor-pointer"
                >
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  Publish
                </button>
              )}
              <button
                type="button"
                onClick={() => {
                  if (
                    window.confirm(
                      `Delete ${selectedIds.size} selected ${selectedIds.size === 1 ? "entry" : "entries"}?`,
                    )
                  ) {
                    bulkDeleteMut.mutate();
                  }
                }}
                disabled={bulkDeleteMut.isPending}
                className="inline-flex h-8 items-center gap-1.5 rounded-[4px] border border-[#f5c0b8] bg-[#fcecea] px-3 text-xs font-semibold text-[#d02b20] transition-colors hover:bg-[#f9d7d3] disabled:opacity-50 cursor-pointer"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Delete
              </button>
            </div>
          )}
        </div>

        {/* Right Gear Button: Displayed Fields Customizer */}
        <DropdownMenu.Root>
          <Tooltip content="Configure displayed fields">
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label="Configure displayed fields"
                className="inline-flex h-8 w-8 items-center justify-center rounded-[4px] border border-[#dcdce4] bg-white text-[#32324d] transition-colors hover:border-[#c0c0cf] hover:bg-[#f6f6f9] cursor-pointer"
              >
                <Settings className="h-3.5 w-3.5 text-[#666687]" strokeWidth={2} />
              </button>
            </DropdownMenu.Trigger>
          </Tooltip>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              align="end"
              sideOffset={6}
              className="z-50 w-64 rounded-[4px] border border-[#eaeaef] bg-white p-3 shadow-lg"
            >
              <div className="mb-2 flex items-center justify-between">
                <span className="text-xs font-bold text-[#32324d]">Displayed fields</span>
                <button
                  type="button"
                  onClick={resetColumns}
                  className="text-[11px] font-semibold text-[#4945ff] hover:underline cursor-pointer"
                >
                  Reset
                </button>
              </div>
              {displayableFields.length === 0 ? (
                <p className="py-2 text-xs text-[#8e8ea9]">No scalar fields on this schema.</p>
              ) : (
                <div className="max-h-60 space-y-1 overflow-y-auto">
                  {displayableFields.map((field) => {
                    const checked = visibleFieldNames.includes(field.name);
                    return (
                      <label
                        key={field.name}
                        htmlFor={`cm-col-${field.name}`}
                        className="flex cursor-pointer items-center gap-2.5 rounded-[4px] px-2 py-1.5 text-xs text-[#32324d] hover:bg-[#f6f6f9]"
                      >
                        <Checkbox
                          id={`cm-col-${field.name}`}
                          checked={checked}
                          onCheckedChange={() => toggleColumnVisibility(field.name)}
                          className="border-[#c0c0cf] data-[state=checked]:border-[#4945ff] data-[state=checked]:bg-[#4945ff]"
                        />
                        <span className="truncate font-medium">{field.name}</span>
                        <span className="ml-auto font-mono text-[10px] text-[#8e8ea9]">
                          {field.fieldType}
                        </span>
                      </label>
                    );
                  })}
                </div>
              )}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      </div>

      {/* Main White Data Table Card */}
      <div className="overflow-hidden rounded-[4px] border border-[#eaeaef] bg-white shadow-[0_1px_4px_rgba(33,33,52,0.06)]">
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-[#eaeaef] bg-white">
                {/* Master Select Checkbox */}
                <th className="w-12 py-3.5 pr-2 pl-5">
                  <Checkbox
                    checked={allSelected}
                    onCheckedChange={(checked) => toggleSelectAll(Boolean(checked))}
                    aria-label="Select all entries"
                    className="h-[18px] w-[18px] rounded-[4px] border-[#c0c0cf] data-[state=checked]:border-[#4945ff] data-[state=checked]:bg-[#4945ff]"
                  />
                </th>

                {/* ID Column */}
                <th className="w-20 px-3 py-3.5">
                  <button
                    type="button"
                    onClick={() => handleSortColumn("id")}
                    className="inline-flex items-center gap-1 text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase hover:text-[#32324d] cursor-pointer"
                  >
                    <span>ID</span>
                    {sortField === "id" &&
                      (sortDir === "asc" ? (
                        <ArrowUp className="h-3 w-3 text-[#4945ff]" />
                      ) : (
                        <ArrowDown className="h-3 w-3 text-[#4945ff]" />
                      ))}
                  </button>
                </th>

                {/* Dynamic Schema Field Columns */}
                {activeColumns.map((field) => (
                  <th key={field.name} className="px-4 py-3.5">
                    <button
                      type="button"
                      onClick={() => handleSortColumn(field.name)}
                      className="inline-flex items-center gap-1 text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase hover:text-[#32324d] cursor-pointer"
                    >
                      <span>{field.name}</span>
                      {sortField === field.name &&
                        (sortDir === "asc" ? (
                          <ArrowUp className="h-3 w-3 text-[#4945ff]" />
                        ) : (
                          <ArrowDown className="h-3 w-3 text-[#4945ff]" />
                        ))}
                    </button>
                  </th>
                ))}

                {/* STATUS Column */}
                {activeSchema.draftAndPublish && (
                  <th className="w-36 px-4 py-3.5 text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase">
                    Status
                  </th>
                )}

                {/* Row Actions Column */}
                <th className="w-14 py-3.5 pr-5 pl-2 text-right">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>

            <tbody className="divide-y divide-[#eaeaef]">
              {entriesQuery.isLoading ? (
                [1, 2, 3, 4, 5].map((skeletonRow) => (
                  <tr key={skeletonRow}>
                    <td
                      colSpan={activeColumns.length + (activeSchema.draftAndPublish ? 4 : 3)}
                      className="px-5 py-4"
                    >
                      <div className="h-5 animate-pulse rounded bg-[#f6f6f9]" />
                    </td>
                  </tr>
                ))
              ) : rows.length === 0 ? (
                <tr>
                  <td
                    colSpan={activeColumns.length + (activeSchema.draftAndPublish ? 4 : 3)}
                    className="px-6 py-16 text-center"
                  >
                    <p className="text-[14px] font-medium text-[#32324d]">No content found</p>
                    <p className="mt-1 text-[13px] text-[#666687]">
                      {search || statusFilter !== "all"
                        ? "Try clearing your search or status filters."
                        : `Click "Create new entry" to add your first ${activeSchema.displayName}.`}
                    </p>
                  </td>
                </tr>
              ) : (
                rows.map((row) => {
                  const editPath = `/content-manager/${encodeURIComponent(activeSchema.uid)}/${encodeURIComponent(row.documentId)}`;
                  const pubState =
                    row.publicationState ?? (row.publishedAt ? "published" : "draft");
                  const isPublished = pubState === "published" || pubState === "modified";
                  const isRowSelected = selectedIds.has(row.documentId);

                  return (
                    <tr
                      key={row.documentId}
                      onClick={() => navigate(editPath)}
                      className={cn(
                        "group cursor-pointer transition-colors",
                        isRowSelected ? "bg-[#f0f0ff]/45" : "hover:bg-[#fafafd]",
                      )}
                    >
                      {/* Row Checkbox */}
                      <td
                        className="py-4 pr-2 pl-5"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <Checkbox
                          checked={isRowSelected}
                          onCheckedChange={(checked) => {
                            const next = new Set(selectedIds);
                            if (checked) next.add(row.documentId);
                            else next.delete(row.documentId);
                            setSelectedIds(next);
                          }}
                          aria-label={`Select entry ${row.id}`}
                          className="h-[18px] w-[18px] rounded-[4px] border-[#c0c0cf] data-[state=checked]:border-[#4945ff] data-[state=checked]:bg-[#4945ff]"
                        />
                      </td>

                      {/* Underlined Numeric ID (matches Strapi screenshot) */}
                      <td className="px-3 py-4 text-[13px] text-[#32324d]">
                        <Link
                          to={editPath}
                          onClick={(e) => e.stopPropagation()}
                          className="tabular underline decoration-[#8e8ea9] underline-offset-2 hover:text-[#4945ff] hover:decoration-[#4945ff]"
                        >
                          {row.id}
                        </Link>
                      </td>

                      {/* Dynamic Field Cells */}
                      {activeColumns.map((field) => {
                        const val = row[field.name];
                        const text = formatCellText(val);
                        return (
                          <td
                            key={field.name}
                            className="max-w-[280px] truncate px-4 py-4 text-[13px] text-[#32324d]"
                          >
                            {text}
                          </td>
                        );
                      })}

                      {/* Status Badge */}
                      {activeSchema.draftAndPublish && (
                        <td className="px-4 py-4">
                          {pubState === "published" ? (
                            <span className="inline-flex items-center rounded-[4px] border border-[#c6f0c2] bg-[#eafbe7] px-2.5 py-1 text-xs font-semibold text-[#2f6846]">
                              Published
                            </span>
                          ) : pubState === "modified" ? (
                            <span className="inline-flex items-center rounded-[4px] border border-[#f5c0b8] bg-[#fdf4dc] px-2.5 py-1 text-xs font-semibold text-[#be5d01]">
                              Modified
                            </span>
                          ) : (
                            <span className="inline-flex items-center rounded-[4px] border border-[#bae0ff] bg-[#eaf5ff] px-2.5 py-1 text-xs font-semibold text-[#0c75af]">
                              Draft
                            </span>
                          )}
                        </td>
                      )}

                      {/* Three-Dot Row Actions Menu */}
                      <td
                        className="py-4 pr-5 pl-2 text-right"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <DropdownMenu.Root>
                          <DropdownMenu.Trigger asChild>
                            <button
                              type="button"
                              aria-label="Entry actions"
                              className="inline-flex h-7 w-7 items-center justify-center rounded-[4px] text-[#666687] transition-colors hover:bg-[#f0f0ff] hover:text-[#4945ff] cursor-pointer"
                            >
                              <MoreHorizontal className="h-4 w-4" />
                            </button>
                          </DropdownMenu.Trigger>
                          <DropdownMenu.Portal>
                            <DropdownMenu.Content
                              align="end"
                              sideOffset={4}
                              className="z-50 w-44 rounded-[4px] border border-[#eaeaef] bg-white p-1 shadow-lg"
                            >
                              <DropdownMenu.Item
                                onSelect={() => navigate(editPath)}
                                className="flex cursor-pointer items-center gap-2 rounded-[4px] px-2.5 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                              >
                                <Pencil className="h-3.5 w-3.5" />
                                <span>Edit</span>
                              </DropdownMenu.Item>

                              <DropdownMenu.Item
                                onSelect={() => rowDuplicateMut.mutate(row)}
                                className="flex cursor-pointer items-center gap-2 rounded-[4px] px-2.5 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                              >
                                <Copy className="h-3.5 w-3.5" />
                                <span>Duplicate</span>
                              </DropdownMenu.Item>

                              {activeSchema.draftAndPublish && (
                                <DropdownMenu.Item
                                  onSelect={() =>
                                    isPublished
                                      ? rowUnpublishMut.mutate(row.documentId)
                                      : rowPublishMut.mutate(row.documentId)
                                  }
                                  className="flex cursor-pointer items-center gap-2 rounded-[4px] px-2.5 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                                >
                                  {isPublished ? (
                                    <>
                                      <XCircle className="h-3.5 w-3.5 text-[#be5d01]" />
                                      <span>Unpublish</span>
                                    </>
                                  ) : (
                                    <>
                                      <CheckCircle2 className="h-3.5 w-3.5 text-[#2f6846]" />
                                      <span>Publish</span>
                                    </>
                                  )}
                                </DropdownMenu.Item>
                              )}

                              <div className="my-1 h-px bg-[#eaeaef]" />

                              <DropdownMenu.Item
                                onSelect={() => {
                                  if (window.confirm("Delete this entry permanently?")) {
                                    rowDeleteMut.mutate(row.documentId);
                                  }
                                }}
                                className="flex cursor-pointer items-center gap-2 rounded-[4px] px-2.5 py-1.5 text-xs font-medium text-[#d02b20] outline-none hover:bg-[#fcecea]"
                              >
                                <Trash2 className="h-3.5 w-3.5" />
                                <span>Delete</span>
                              </DropdownMenu.Item>
                            </DropdownMenu.Content>
                          </DropdownMenu.Portal>
                        </DropdownMenu.Root>
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* Pagination Footer (shown when multiple pages or page-size selector needed) */}
      {pagination && (pagination.pageCount > 1 || totalCount > 10) && (
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3 text-xs text-[#666687]">
          <div className="flex items-center gap-2">
            <select
              value={pageSize}
              onChange={(e) => {
                setPageSize(Number(e.target.value));
                setPage(1);
              }}
              aria-label="Entries per page"
              className="h-8 rounded-[4px] border border-[#dcdce4] bg-white px-2 text-xs font-medium text-[#32324d] focus:border-[#4945ff] focus:outline-none"
            >
              {[10, 20, 50, 100].map((size) => (
                <option key={size} value={size}>
                  {size}
                </option>
              ))}
            </select>
            <span>Entries per page</span>
          </div>

          <div className="flex items-center gap-1.5">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              aria-label="Previous page"
              className="inline-flex h-8 w-8 items-center justify-center rounded-[4px] border border-[#dcdce4] bg-white text-[#32324d] transition-colors hover:bg-[#f6f6f9] disabled:pointer-events-none disabled:opacity-40 cursor-pointer"
            >
              <ChevronLeft className="h-4 w-4" />
            </button>
            <span className="px-2 font-medium text-[#32324d]">
              Page {pagination.page} of {pagination.pageCount}
            </span>
            <button
              type="button"
              disabled={page >= pagination.pageCount}
              onClick={() => setPage((p) => p + 1)}
              aria-label="Next page"
              className="inline-flex h-8 w-8 items-center justify-center rounded-[4px] border border-[#dcdce4] bg-white text-[#32324d] transition-colors hover:bg-[#f6f6f9] disabled:pointer-events-none disabled:opacity-40 cursor-pointer"
            >
              <ChevronRight className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
