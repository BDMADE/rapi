import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  CheckCircle2,
  Copy,
  Diff,
  ExternalLink,
  Eye,
  GitPullRequest,
  Globe,
  History,
  Rocket,
  RotateCcw,
  Save,
  Sparkles,
  Trash2,
  XCircle,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useUiStore } from "@/app/store";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Dialog,
  DialogContent,
  Input,
  Select,
  Switch,
  useToast,
} from "@/components/ui";
import { useAuthStore } from "@/features/auth/auth-store";
import { BlocksEditor } from "@/features/content-manager/blocks-editor";
import {
  ComponentFieldEditor,
  DynamicZoneEditor,
} from "@/features/content-manager/dynamic-zone-editor";
import { RelationPicker } from "@/features/content-manager/relation-picker";
import { MediaPicker } from "@/features/media/media-picker";
import {
  api,
  type ContentTypeSchema,
  type ContentVersionDto,
  type FieldDefinition,
} from "@/lib/api";

function isFieldVisible(field: FieldDefinition, values: Record<string, unknown>): boolean {
  const rule = field.validation?.conditions?.visible;
  if (!rule || !rule.field) return true;
  const current = values[rule.field];
  if (rule.eq !== undefined) return current === rule.eq;
  if (rule.ne !== undefined) return current !== rule.ne;
  return true;
}

function JsonFieldEditor({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (val: unknown) => void;
}) {
  const [text, setText] = useState(() =>
    value !== undefined && value !== null ? JSON.stringify(value, null, 2) : "{}",
  );
  const [parseError, setParseError] = useState<string | null>(null);

  useEffect(() => {
    setText(value !== undefined && value !== null ? JSON.stringify(value, null, 2) : "{}");
    setParseError(null);
  }, [value]);

  return (
    <div>
      <textarea
        rows={4}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          try {
            const parsed = JSON.parse(e.target.value);
            setParseError(null);
            onChange(parsed);
          } catch (err) {
            setParseError(err instanceof Error ? err.message : "Invalid JSON");
          }
        }}
        className="w-full rounded-md border border-zinc-300 bg-zinc-950 p-2.5 font-mono text-[11px] text-zinc-100 focus:border-indigo-600 focus:outline-none"
      />
      {parseError && <p className="mt-1 text-[11px] text-rose-600">{parseError}</p>}
    </div>
  );
}

function slugifyUid(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function EntryEditorPage() {
  const { uid, documentId } = useParams();
  const navigate = useNavigate();
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const activeLocale = useUiStore((s) => s.activeLocale);
  const setActiveLocale = useUiStore((s) => s.setActiveLocale);
  const authToken = useAuthStore((s) => s.token);

  const isCreate = documentId === "create";
  const isSingle = documentId === "single";

  const catalogQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });

  const schema = useMemo(
    () => catalogQuery.data?.contentTypes.find((s) => s.uid === uid),
    [catalogQuery.data, uid],
  );

  const componentsByUid = useMemo(() => {
    const map = new Map<string, ContentTypeSchema>();
    for (const c of catalogQuery.data?.components ?? []) {
      map.set(c.uid, c);
    }
    return map;
  }, [catalogQuery.data]);

  const contentTypesByUid = useMemo(() => {
    const map = new Map<string, ContentTypeSchema>();
    for (const ct of catalogQuery.data?.contentTypes ?? []) {
      map.set(ct.uid, ct);
    }
    return map;
  }, [catalogQuery.data]);

  const entryQuery = useQuery({
    queryKey: ["cm-entry", uid, documentId, activeLocale],
    queryFn: async () => {
      try {
        return isSingle
          ? await api.getSingleEntry(uid!, activeLocale)
          : await api.getEntry(uid!, documentId!, activeLocale);
      } catch {
        // Unauthored locale variant of an existing documentId returns null so user can author or fill-from-locale
        return null;
      }
    },
    enabled: Boolean(uid && !isCreate),
  });

  const resolvedDocId = isCreate
    ? null
    : (entryQuery.data?.documentId ?? (isSingle ? null : documentId ?? null));

  const versionsQuery = useQuery({
    queryKey: ["cm-versions", uid, resolvedDocId, activeLocale],
    queryFn: () => api.listEntryVersions(uid!, resolvedDocId!, activeLocale),
    enabled: Boolean(uid && resolvedDocId),
  });

  const localesQuery = useQuery({
    queryKey: ["locales"],
    queryFn: api.listLocales,
  });

  const docLocalesQuery = useQuery({
    queryKey: ["cm-doc-locales", uid, resolvedDocId],
    queryFn: () => api.listDocumentLocales(uid!, resolvedDocId!),
    enabled: Boolean(uid && resolvedDocId && schema?.i18nEnabled),
  });

  const workflowStagesQuery = useQuery({
    queryKey: ["workflow-stages"],
    queryFn: api.listWorkflowStages,
  });

  const workflowEntryQuery = useQuery({
    queryKey: ["workflow-entry", resolvedDocId, activeLocale],
    queryFn: () => api.getWorkflowEntry(resolvedDocId!, activeLocale),
    enabled: Boolean(resolvedDocId),
  });

  const adminUsersQuery = useQuery({
    queryKey: ["admin-users"],
    queryFn: api.listAdminUsers,
  });

  const releasesQuery = useQuery({
    queryKey: ["releases"],
    queryFn: api.listReleases,
    enabled: Boolean(resolvedDocId),
  });

  const [formValues, setFormValues] = useState<Record<string, unknown>>({});
  const [copiedId, setCopiedId] = useState(false);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [previewTick, setPreviewTick] = useState(0);
  const [diffVersion, setDiffVersion] = useState<ContentVersionDto | null>(null);
  const [selectedReleaseId, setSelectedReleaseId] = useState("");
  const [fillSourceLocale, setFillSourceLocale] = useState("");

  useEffect(() => {
    if (isCreate) {
      setFormValues({});
    } else if (entryQuery.data) {
      const next: Record<string, unknown> = {};
      for (const f of schema?.fields ?? []) {
        if (f.name in entryQuery.data) {
          next[f.name] = entryQuery.data[f.name];
        }
      }
      setFormValues(next);
    } else if (entryQuery.isSuccess && !entryQuery.data) {
      setFormValues({});
    }
  }, [isCreate, entryQuery.data, entryQuery.isSuccess, schema]);

  // Subscribe to SSE `/api/preview/events` when Live Preview split pane is open
  useEffect(() => {
    if (!previewOpen || !authToken || !resolvedDocId) return;
    const es = new EventSource(api.getPreviewSseUrl(authToken));
    const onChanged = (ev: MessageEvent) => {
      try {
        const data = JSON.parse(ev.data) as { uid?: string; documentId?: string };
        if (data.documentId === resolvedDocId) {
          setPreviewTick((t) => t + 1);
        }
      } catch {
        setPreviewTick((t) => t + 1);
      }
    };
    es.addEventListener("document.changed", onChanged);
    return () => {
      es.removeEventListener("document.changed", onChanged);
      es.close();
    };
  }, [previewOpen, authToken, resolvedDocId]);

  // Automatically keep the split Live Preview iframe synchronized when switching locales
  useEffect(() => {
    if (!previewOpen || !uid || !resolvedDocId) return;
    let cancelled = false;
    api
      .resolvePreviewUrl(
        uid,
        resolvedDocId,
        activeLocale,
        schema?.draftAndPublish ? "draft" : "published",
      )
      .then((resolved) => {
        if (cancelled) return;
        const finalUrl =
          resolved.isBuiltin && authToken
            ? `${resolved.url}&token=${encodeURIComponent(authToken)}`
            : resolved.url;
        setPreviewUrl(finalUrl);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [previewOpen, uid, resolvedDocId, activeLocale, authToken, schema?.draftAndPublish]);

  const setField = (name: string, val: unknown) => {
    setFormValues((prev) => ({ ...prev, [name]: val }));
  };

  const saveMut = useMutation({
    mutationFn: async () => {
      if (!uid || !schema) throw new Error("Schema missing");
      if (isSingle) {
        return api.upsertSingleEntry(uid, formValues, activeLocale);
      }
      if (isCreate) {
        return api.createEntry(uid, formValues, activeLocale);
      }
      return api.updateEntry(uid, documentId!, formValues, activeLocale);
    },
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["cm-entry"] });
      queryClient.invalidateQueries({ queryKey: ["cm-versions"] });
      queryClient.invalidateQueries({ queryKey: ["cm-doc-locales"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      setPreviewTick((t) => t + 1);
      pushToast({ title: "Saved changes", tone: "success" });
      if (isCreate && saved.documentId) {
        navigate(
          `/content-manager/${encodeURIComponent(uid!)}/${encodeURIComponent(saved.documentId)}`,
          { replace: true },
        );
      }
    },
    onError: (e: Error) =>
      pushToast({ title: "Save failed", description: e.message, tone: "error" }),
  });

  const publishMut = useMutation({
    mutationFn: () => api.publishEntry(uid!, resolvedDocId!, activeLocale),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["cm-entry"] });
      queryClient.invalidateQueries({ queryKey: ["cm-versions"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Published", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Publish failed", description: e.message, tone: "error" }),
  });

  const unpublishMut = useMutation({
    mutationFn: () => api.unpublishEntry(uid!, resolvedDocId!, activeLocale),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["cm-entry"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Unpublished", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Unpublish failed", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: () => api.deleteEntry(uid!, resolvedDocId!),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Document deleted", tone: "success" });
      navigate(`/content-manager/${encodeURIComponent(uid!)}`);
    },
    onError: (e: Error) =>
      pushToast({ title: "Delete failed", description: e.message, tone: "error" }),
  });

  const restoreMut = useMutation({
    mutationFn: (versionId: string) =>
      api.restoreEntryVersion(uid!, resolvedDocId!, versionId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["cm-entry"] });
      queryClient.invalidateQueries({ queryKey: ["cm-versions"] });
      setDiffVersion(null);
      setPreviewTick((t) => t + 1);
      pushToast({ title: "Historical version restored to draft", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Restore failed", description: e.message, tone: "error" }),
  });

  const fillLocaleMut = useMutation({
    mutationFn: (sourceLocale: string) =>
      api.fillFromLocale(uid!, resolvedDocId!, sourceLocale, activeLocale),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["cm-entry"] });
      queryClient.invalidateQueries({ queryKey: ["cm-doc-locales"] });
      queryClient.invalidateQueries({ queryKey: ["cm-versions"] });
      pushToast({ title: `Filled '${activeLocale}' fields from source locale`, tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Fill locale failed", description: e.message, tone: "error" }),
  });

  const workflowMut = useMutation({
    mutationFn: (payload: { stageId: string; assigneeId: string | null }) =>
      api.updateWorkflowEntry(resolvedDocId!, {
        locale: activeLocale,
        stageId: payload.stageId,
        assigneeId: payload.assigneeId,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workflow-entry"] });
      pushToast({ title: "Updated review workflow", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Workflow restricted", description: e.message, tone: "error" }),
  });

  const addToReleaseMut = useMutation({
    mutationFn: (releaseId: string) =>
      api.addReleaseEntry(releaseId, {
        contentTypeUid: uid!,
        documentId: resolvedDocId!,
        locale: activeLocale,
        action: "publish",
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["releases"] });
      setSelectedReleaseId("");
      pushToast({ title: "Added entry to release", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Release error", description: e.message, tone: "error" }),
  });

  const handleTogglePreview = async () => {
    if (!uid || !resolvedDocId) return;
    if (previewOpen) {
      setPreviewOpen(false);
      return;
    }
    try {
      const resolved = await api.resolvePreviewUrl(
        uid,
        resolvedDocId,
        activeLocale,
        schema?.draftAndPublish ? "draft" : "published",
      );
      const finalUrl =
        resolved.isBuiltin && authToken
          ? `${resolved.url}&token=${encodeURIComponent(authToken)}`
          : resolved.url;
      if (resolved.openMode === "tab" && !resolved.isBuiltin) {
        window.open(finalUrl, "_blank", "noopener,noreferrer");
      } else {
        setPreviewUrl(finalUrl);
        setPreviewOpen(true);
      }
    } catch (e) {
      pushToast({
        title: "Preview unavailable",
        description: e instanceof Error ? e.message : "Save document first",
        tone: "error",
      });
    }
  };

  if (!schema) {
    return <p className="text-xs text-zinc-500">Loading schema...</p>;
  }

  const pubState =
    entryQuery.data?.publicationState ??
    (entryQuery.data?.publishedAt ? "published" : "draft");
  const isPublished = pubState === "published" || pubState === "modified";

  return (
    <div className="space-y-6">
      {/* Header bar */}
      <div>
        {!isSingle && (
          <Link
            to={`/content-manager/${encodeURIComponent(schema.uid)}`}
            className="mb-2.5 inline-flex items-center gap-1.5 text-[13px] font-medium text-[#4945ff] transition-colors hover:text-[#271fe0]"
          >
            <ArrowLeft className="h-3.5 w-3.5" strokeWidth={2.2} />
            <span>Back</span>
          </Link>
        )}
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="flex items-center gap-2.5">
              <h1 className="text-[28px] leading-[1.22] font-bold tracking-[-0.02em] text-[#32324d]">
                {isCreate ? `Create an entry` : schema.displayName}
              </h1>
              <Badge tone="mono">{activeLocale}</Badge>
              {!isCreate && schema.draftAndPublish && (
                <span
                  className={
                    pubState === "published"
                      ? "inline-flex items-center rounded-[4px] border border-[#c6f0c2] bg-[#eafbe7] px-2.5 py-0.5 text-xs font-semibold text-[#2f6846]"
                      : pubState === "modified"
                        ? "inline-flex items-center rounded-[4px] border border-[#f5c0b8] bg-[#fdf4dc] px-2.5 py-0.5 text-xs font-semibold text-[#be5d01]"
                        : "inline-flex items-center rounded-[4px] border border-[#bae0ff] bg-[#eaf5ff] px-2.5 py-0.5 text-xs font-semibold text-[#0c75af]"
                  }
                >
                  {pubState === "published"
                    ? "Published"
                    : pubState === "modified"
                      ? "Modified"
                      : "Draft"}
                </span>
              )}
            </div>
            <p className="mt-1 text-[13px] text-[#666687]">API ID : {schema.singularName}</p>
          </div>

          <div className="flex items-center gap-2">
            {resolvedDocId && entryQuery.data && (
              <Button
                size="sm"
                variant={previewOpen ? "primary" : "secondary"}
                onClick={handleTogglePreview}
              >
                <Eye className="h-3.5 w-3.5" />
                {previewOpen ? "Close preview" : "Live preview"}
              </Button>
            )}
            <Button
              size="sm"
              variant={schema.draftAndPublish ? "secondary" : "primary"}
              onClick={() => saveMut.mutate()}
              disabled={saveMut.isPending}
              className={
                !schema.draftAndPublish ? "border-[#4945ff] bg-[#4945ff] hover:bg-[#271fe0]" : ""
              }
            >
              <Save className="h-3.5 w-3.5" />
              {saveMut.isPending ? "Saving..." : "Save"}
            </Button>

            {schema.draftAndPublish && resolvedDocId && (
              <>
                <Button
                  size="sm"
                  onClick={() => publishMut.mutate()}
                  disabled={publishMut.isPending}
                  className="border-[#4945ff] bg-[#4945ff] hover:bg-[#271fe0]"
                >
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  Publish
                </Button>
                {isPublished && (
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => unpublishMut.mutate()}
                    disabled={unpublishMut.isPending}
                  >
                    <XCircle className="h-3.5 w-3.5 text-amber-600" />
                    Unpublish
                  </Button>
                )}
              </>
            )}
          </div>
        </div>
      </div>

      {/* Two-column editor (or split Live Preview) + right inspector rail */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        {/* Form canvas */}
        <Card className={previewOpen ? "lg:col-span-5" : "lg:col-span-8"}>
          <CardContent className="space-y-4">
            {schema.fields.length === 0 ? (
              <p className="text-xs text-zinc-500">
                This content type has no fields yet. Add fields in Content-Type Builder.
              </p>
            ) : (
              schema.fields
                .filter((field) => isFieldVisible(field, formValues))
                .map((field) => (
                  <FieldRenderer
                    key={field.name}
                    field={field}
                    value={formValues[field.name]}
                    allValues={formValues}
                    locale={activeLocale}
                    componentsByUid={componentsByUid}
                    contentTypesByUid={contentTypesByUid}
                    onChange={(val) => setField(field.name, val)}
                  />
                ))
            )}
          </CardContent>
        </Card>

        {previewOpen && previewUrl && (
          <Card className="lg:col-span-4">
            <CardHeader className="flex flex-row items-center justify-between py-2.5">
              <div className="flex items-center gap-1.5">
                <Eye className="h-3.5 w-3.5 text-indigo-700" />
                <CardTitle>Live Preview (SSE)</CardTitle>
              </div>
              <a
                href={previewUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-[11px] font-medium text-indigo-700 hover:underline"
              >
                Open
                <ExternalLink className="h-3 w-3" />
              </a>
            </CardHeader>
            <div className="h-[540px] border-t border-zinc-200 bg-zinc-50">
              <iframe
                key={`${previewUrl}-${previewTick}`}
                src={previewUrl}
                title="Live Document Preview"
                className="h-full w-full border-0"
              />
            </div>
          </Card>
        )}

        {/* Right rail: Document info + i18n + Workflow + Releases + Version History */}
        <div className={previewOpen ? "space-y-4 lg:col-span-3" : "space-y-4 lg:col-span-4"}>
          <Card>
            <CardHeader>
              <CardTitle>Document metadata</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2.5 text-xs">
              <div className="flex items-center justify-between">
                <span className="text-zinc-500">Document ID</span>
                {resolvedDocId ? (
                  <button
                    type="button"
                    onClick={() => {
                      navigator.clipboard.writeText(resolvedDocId);
                      setCopiedId(true);
                      setTimeout(() => setCopiedId(false), 1500);
                    }}
                    className="inline-flex items-center gap-1 font-mono text-[11px] text-zinc-800 hover:text-indigo-700 cursor-pointer"
                  >
                    {resolvedDocId.slice(0, 12)}…
                    {copiedId ? (
                      <Check className="h-3 w-3 text-emerald-600" />
                    ) : (
                      <Copy className="h-3 w-3" />
                    )}
                  </button>
                ) : (
                  <span className="text-zinc-400">Assigned on save</span>
                )}
              </div>
              <div className="flex items-center justify-between">
                <span className="text-zinc-500">Locale</span>
                <Badge tone="mono">{activeLocale}</Badge>
              </div>
              {entryQuery.data?.updatedAt && (
                <div className="flex items-center justify-between">
                  <span className="text-zinc-500">Last updated</span>
                  <span className="font-mono text-[11px] text-zinc-700">
                    {new Date(entryQuery.data.updatedAt).toLocaleString()}
                  </span>
                </div>
              )}
              {resolvedDocId && !isSingle && (
                <div className="border-t border-zinc-200 pt-2.5">
                  <Button
                    type="button"
                    size="sm"
                    variant="danger"
                    className="w-full"
                    disabled={deleteMut.isPending}
                    onClick={() => {
                      if (window.confirm("Delete this document permanently?")) {
                        deleteMut.mutate();
                      }
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    Delete document
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>

          {resolvedDocId && schema.i18nEnabled && (
            <Card>
              <CardHeader>
                <div className="flex items-center gap-1.5">
                  <Globe className="h-3.5 w-3.5 text-indigo-700" />
                  <CardTitle>Internationalization</CardTitle>
                </div>
              </CardHeader>
              <CardContent className="space-y-2.5 text-xs">
                <div className="flex flex-wrap gap-1.5">
                  {(localesQuery.data ?? []).map((loc) => {
                    const st = (docLocalesQuery.data ?? []).find((d) => d.locale === loc.code);
                    const isCurrent = loc.code === activeLocale;
                    return (
                      <button
                        key={loc.code}
                        type="button"
                        onClick={() => setActiveLocale(loc.code)}
                        className={`inline-flex items-center gap-1 rounded border px-2 py-1 font-mono text-[11px] cursor-pointer ${
                          isCurrent
                            ? "border-indigo-600 bg-indigo-50 font-semibold text-indigo-900"
                            : "border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50"
                        }`}
                      >
                        <span>{loc.code}</span>
                        <span
                          className={`h-1.5 w-1.5 rounded-full ${
                            st?.hasPublished
                              ? "bg-emerald-600"
                              : st?.hasDraft
                                ? "bg-amber-500"
                                : "bg-zinc-300"
                          }`}
                        />
                      </button>
                    );
                  })}
                </div>
                {(docLocalesQuery.data ?? []).some((d) => d.locale !== activeLocale) && (
                  <div className="flex items-center gap-1.5 pt-1">
                    <div className="flex-1">
                      <Select
                        value={fillSourceLocale}
                        onValueChange={setFillSourceLocale}
                        options={[
                          { value: "", label: "Fill from locale..." },
                          ...(docLocalesQuery.data ?? [])
                            .filter((d) => d.locale !== activeLocale)
                            .map((d) => ({ value: d.locale, label: `Copy from ${d.locale}` })),
                        ]}
                      />
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      disabled={!fillSourceLocale || fillLocaleMut.isPending}
                      onClick={() => fillLocaleMut.mutate(fillSourceLocale)}
                    >
                      Fill
                    </Button>
                  </div>
                )}
              </CardContent>
            </Card>
          )}

          {resolvedDocId && (
            <Card>
              <CardHeader>
                <div className="flex items-center gap-1.5">
                  <GitPullRequest className="h-3.5 w-3.5 text-indigo-700" />
                  <CardTitle>Review workflow</CardTitle>
                </div>
              </CardHeader>
              <CardContent className="space-y-2.5 text-xs">
                <div>
                  <label className="mb-1 block text-[11px] text-zinc-500">Stage</label>
                  <Select
                    value={workflowEntryQuery.data?.stageId ?? ""}
                    onValueChange={(stageId) =>
                      workflowMut.mutate({
                        stageId,
                        assigneeId: workflowEntryQuery.data?.assigneeId ?? null,
                      })
                    }
                    options={(workflowStagesQuery.data ?? []).map((s) => ({
                      value: s.id,
                      label: s.requireForPublish ? `${s.name} (Publish Gate)` : s.name,
                    }))}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-[11px] text-zinc-500">Assignee</label>
                  <Select
                    value={workflowEntryQuery.data?.assigneeId ?? "__none__"}
                    onValueChange={(val) => {
                      const stageId =
                        workflowEntryQuery.data?.stageId ??
                        workflowStagesQuery.data?.[0]?.id;
                      if (!stageId) return;
                      workflowMut.mutate({
                        stageId,
                        assigneeId: val === "__none__" ? null : val,
                      });
                    }}
                    options={[
                      { value: "__none__", label: "Unassigned" },
                      ...(adminUsersQuery.data ?? []).map((u) => ({
                        value: u.id,
                        label: `${u.firstname || u.email} (${u.email})`,
                      })),
                    ]}
                  />
                </div>
              </CardContent>
            </Card>
          )}

          {resolvedDocId && schema.draftAndPublish && (
            <Card>
              <CardHeader>
                <div className="flex items-center gap-1.5">
                  <Rocket className="h-3.5 w-3.5 text-indigo-700" />
                  <CardTitle>Add to release</CardTitle>
                </div>
              </CardHeader>
              <CardContent className="space-y-2 text-xs">
                {(releasesQuery.data ?? []).filter((r) => r.status !== "done").length === 0 ? (
                  <p className="text-zinc-500">
                    No pending releases.{" "}
                    <Link to="/settings/releases" className="text-indigo-700 underline">
                      Create a release
                    </Link>
                  </p>
                ) : (
                  <div className="flex items-center gap-1.5">
                    <div className="flex-1">
                      <Select
                        value={selectedReleaseId}
                        onValueChange={setSelectedReleaseId}
                        options={[
                          { value: "", label: "Select pending release..." },
                          ...(releasesQuery.data ?? [])
                            .filter((r) => r.status !== "done")
                            .map((r) => ({ value: r.id, label: r.name })),
                        ]}
                      />
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      disabled={!selectedReleaseId || addToReleaseMut.isPending}
                      onClick={() => addToReleaseMut.mutate(selectedReleaseId)}
                    >
                      Add
                    </Button>
                  </div>
                )}
              </CardContent>
            </Card>
          )}

          {resolvedDocId && (
            <Card>
              <CardHeader>
                <div className="flex items-center gap-1.5">
                  <History className="h-3.5 w-3.5 text-indigo-700" />
                  <CardTitle>
                    Content history ({versionsQuery.data?.length ?? 0})
                  </CardTitle>
                </div>
              </CardHeader>
              <div className="max-h-72 divide-y divide-zinc-200 overflow-y-auto text-xs">
                {(versionsQuery.data ?? []).length === 0 ? (
                  <p className="p-4 text-zinc-500">No snapshots recorded yet.</p>
                ) : (
                  (versionsQuery.data ?? []).map((ver) => (
                    <div
                      key={ver.id}
                      className="flex items-center justify-between gap-2 px-4 py-2.5"
                    >
                      <div>
                        <div className="flex items-center gap-1.5">
                          <span className="font-mono font-semibold text-zinc-900">
                            v{ver.version}
                          </span>
                          <Badge
                            tone={ver.status === "published" ? "emerald" : "neutral"}
                          >
                            {ver.status}
                          </Badge>
                        </div>
                        <div className="mt-0.5 font-mono text-[10px] text-zinc-500">
                          {new Date(ver.createdAt).toLocaleString()}
                        </div>
                      </div>
                      <div className="flex items-center gap-1">
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          title="Inspect diff against current form"
                          onClick={() => setDiffVersion(ver)}
                        >
                          <Diff className="h-3.5 w-3.5" />
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          title="Restore this snapshot"
                          disabled={restoreMut.isPending}
                          onClick={() => restoreMut.mutate(ver.id)}
                        >
                          <RotateCcw className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </Card>
          )}
        </div>
      </div>

      {/* Snapshot Diff Dialog */}
      <Dialog open={Boolean(diffVersion)} onOpenChange={(o) => !o && setDiffVersion(null)}>
        <DialogContent
          title={diffVersion ? `Snapshot Diff · v${diffVersion.version} vs Current` : "Diff"}
          className="max-w-2xl"
        >
          {diffVersion && (
            <div className="space-y-3 text-xs">
              <div className="max-h-96 divide-y divide-zinc-200 overflow-y-auto rounded border border-zinc-200">
                {schema.fields
                  .filter((f) => f.fieldType !== "relation" && f.fieldType !== "media")
                  .map((f) => {
                    const snapStr = JSON.stringify(diffVersion.data?.[f.name] ?? null);
                    const curStr = JSON.stringify(formValues[f.name] ?? null);
                    const changed = snapStr !== curStr;
                    return (
                      <div key={f.name} className="space-y-1 p-2.5">
                        <div className="flex items-center justify-between">
                          <span className="font-mono font-semibold text-zinc-800">{f.name}</span>
                          <Badge tone={changed ? "amber" : "neutral"}>
                            {changed ? "changed" : "unchanged"}
                          </Badge>
                        </div>
                        {changed && (
                          <div className="grid grid-cols-2 gap-2 pt-1 font-mono text-[11px]">
                            <div className="rounded border border-amber-200 bg-amber-50 p-2 text-amber-950 break-all">
                              <div className="mb-0.5 text-[10px] font-semibold uppercase text-amber-700">
                                Snapshot v{diffVersion.version}
                              </div>
                              {snapStr}
                            </div>
                            <div className="rounded border border-emerald-200 bg-emerald-50 p-2 text-emerald-950 break-all">
                              <div className="mb-0.5 text-[10px] font-semibold uppercase text-emerald-700">
                                Current Editor
                              </div>
                              {curStr}
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}
              </div>
              <div className="flex justify-end gap-2">
                <Button size="sm" variant="secondary" onClick={() => setDiffVersion(null)}>
                  Close
                </Button>
                <Button
                  size="sm"
                  disabled={restoreMut.isPending}
                  onClick={() => restoreMut.mutate(diffVersion.id)}
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                  Restore v{diffVersion.version}
                </Button>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

function FieldRenderer({
  field,
  value,
  allValues,
  locale,
  componentsByUid,
  contentTypesByUid,
  onChange,
}: {
  field: FieldDefinition;
  value: unknown;
  allValues: Record<string, unknown>;
  locale: string;
  componentsByUid: Map<string, ContentTypeSchema>;
  contentTypesByUid: Map<string, ContentTypeSchema>;
  onChange: (val: unknown) => void;
}) {
  const labelRow = (
    <div className="mb-1 flex items-center justify-between">
      <label className="text-xs font-medium text-zinc-800">
        {field.name}
        {field.required && <span className="ml-0.5 text-rose-600">*</span>}
      </label>
      <div className="flex items-center gap-1">
        <Badge tone="mono">{field.fieldType}</Badge>
        {field.localized && <Badge tone="neutral">{locale}</Badge>}
        {field.private && <Badge tone="rose">private</Badge>}
      </div>
    </div>
  );

  switch (field.fieldType) {
    case "boolean":
      return (
        <div className="flex items-center justify-between rounded-md border border-zinc-200 px-3 py-2.5">
          <div>
            <div className="text-xs font-medium text-zinc-800">{field.name}</div>
            <div className="text-[11px] text-zinc-500">Boolean toggle</div>
          </div>
          <Switch checked={Boolean(value)} onCheckedChange={onChange} />
        </div>
      );

    case "text":
    case "richtext":
      return (
        <div>
          {labelRow}
          <textarea
            rows={field.fieldType === "richtext" ? 6 : 3}
            value={typeof value === "string" ? value : ""}
            onChange={(e) => onChange(e.target.value)}
            placeholder={
              field.fieldType === "richtext" ? "Write Markdown content..." : undefined
            }
            className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-2 text-xs text-zinc-900 focus:border-indigo-600 focus:outline-none"
          />
        </div>
      );

    case "blocks":
      return (
        <div>
          {labelRow}
          <BlocksEditor value={value} onChange={onChange} />
        </div>
      );

    case "enumeration":
      return (
        <div>
          {labelRow}
          <Select
            value={typeof value === "string" ? value : ""}
            onValueChange={onChange}
            options={(field.validation?.enumValues ?? []).map((ev) => ({
              value: ev,
              label: ev,
            }))}
          />
        </div>
      );

    case "uid":
      return (
        <div>
          {labelRow}
          <div className="flex items-center gap-2">
            <Input
              value={typeof value === "string" ? value : ""}
              onChange={(e) => onChange(e.target.value)}
              placeholder="auto-generated-slug"
            />
            {field.validation?.targetField && (
              <Button
                type="button"
                size="sm"
                variant="secondary"
                onClick={() => {
                  const src = allValues[field.validation!.targetField!];
                  if (typeof src === "string") {
                    onChange(slugifyUid(src));
                  }
                }}
              >
                <Sparkles className="h-3.5 w-3.5" />
                Generate
              </Button>
            )}
          </div>
        </div>
      );

    case "integer":
    case "biginteger":
    case "float":
    case "decimal":
      return (
        <div>
          {labelRow}
          <Input
            type="number"
            step={field.fieldType === "float" || field.fieldType === "decimal" ? "any" : "1"}
            value={value === undefined || value === null ? "" : String(value)}
            onChange={(e) => {
              const raw = e.target.value;
              onChange(raw === "" ? null : Number(raw));
            }}
          />
        </div>
      );

    case "date":
    case "datetime":
    case "time":
      return (
        <div>
          {labelRow}
          <Input
            type={
              field.fieldType === "date"
                ? "date"
                : field.fieldType === "time"
                  ? "time"
                  : "datetime-local"
            }
            value={typeof value === "string" ? value : ""}
            onChange={(e) => onChange(e.target.value)}
          />
        </div>
      );

    case "json":
    case "custom":
      return (
        <div>
          {labelRow}
          <JsonFieldEditor value={value} onChange={onChange} />
        </div>
      );

    case "component":
      return (
        <div>
          {labelRow}
          <ComponentFieldEditor
            componentSchema={
              field.component?.componentUid
                ? componentsByUid.get(field.component.componentUid)
                : undefined
            }
            repeatable={Boolean(field.component?.repeatable)}
            value={value}
            onChange={onChange}
          />
        </div>
      );

    case "dynamiczone":
      return (
        <div>
          {labelRow}
          <DynamicZoneEditor
            allowedUids={field.component?.components ?? []}
            componentsByUid={componentsByUid}
            value={value}
            onChange={onChange}
          />
        </div>
      );

    case "relation":
      return (
        <div>
          {labelRow}
          {field.relation?.targetUid ? (
            <RelationPicker
              targetUid={field.relation.targetUid}
              targetSchema={contentTypesByUid.get(field.relation.targetUid)}
              relationKind={field.relation.relationType}
              locale={locale}
              value={value}
              onChange={onChange}
            />
          ) : (
            <p className="text-xs text-zinc-500">Polymorphic relation configured.</p>
          )}
        </div>
      );

    case "media":
      return (
        <div>
          {labelRow}
          <MediaPicker
            multiple={Boolean(field.validation?.multiple)}
            value={value}
            onChange={onChange}
          />
        </div>
      );

    default:
      return (
        <div>
          {labelRow}
          <Input
            type={
              field.fieldType === "password"
                ? "password"
                : field.fieldType === "email"
                  ? "email"
                  : "text"
            }
            value={typeof value === "string" ? value : ""}
            onChange={(e) => onChange(e.target.value)}
          />
        </div>
      );
  }
}
