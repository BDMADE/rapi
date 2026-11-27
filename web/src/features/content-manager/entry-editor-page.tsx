import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  CheckCircle2,
  Copy,
  History,
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
  Input,
  Select,
  Switch,
  useToast,
} from "@/components/ui";
import { BlocksEditor } from "@/features/content-manager/blocks-editor";
import {
  ComponentFieldEditor,
  DynamicZoneEditor,
} from "@/features/content-manager/dynamic-zone-editor";
import { RelationPicker } from "@/features/content-manager/relation-picker";
import { api, type ContentTypeSchema, type FieldDefinition } from "@/lib/api";

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
    queryFn: () =>
      isSingle
        ? api.getSingleEntry(uid!, activeLocale)
        : api.getEntry(uid!, documentId!, activeLocale),
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

  const [formValues, setFormValues] = useState<Record<string, unknown>>({});
  const [copiedId, setCopiedId] = useState(false);

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
    } else if (isSingle && entryQuery.isSuccess && !entryQuery.data) {
      setFormValues({});
    }
  }, [isCreate, isSingle, entryQuery.data, entryQuery.isSuccess, schema]);

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
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
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
      pushToast({ title: "Historical version restored to draft", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Restore failed", description: e.message, tone: "error" }),
  });

  if (!schema) {
    return <p className="text-xs text-zinc-500">Loading schema...</p>;
  }

  const pubState =
    entryQuery.data?.publicationState ??
    (entryQuery.data?.publishedAt ? "published" : "draft");
  const isPublished = pubState === "published" || pubState === "modified";

  return (
    <div className="space-y-5">
      {/* Header bar */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          {!isSingle && (
            <Button size="sm" variant="secondary" asChild>
              <Link to={`/content-manager/${encodeURIComponent(schema.uid)}`}>
                <ArrowLeft className="h-3.5 w-3.5" />
                Back
              </Link>
            </Button>
          )}
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
                {isCreate ? `Create ${schema.displayName}` : schema.displayName}
              </h1>
              <Badge tone="mono">{activeLocale}</Badge>
              {!isCreate && (
                <Badge
                  tone={
                    pubState === "published"
                      ? "emerald"
                      : pubState === "modified"
                        ? "amber"
                        : "neutral"
                  }
                >
                  {pubState}
                </Badge>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant={schema.draftAndPublish ? "secondary" : "primary"}
            onClick={() => saveMut.mutate()}
            disabled={saveMut.isPending}
          >
            <Save className="h-3.5 w-3.5" />
            {saveMut.isPending ? "Saving..." : "Save changes"}
          </Button>

          {schema.draftAndPublish && resolvedDocId && (
            <>
              <Button
                size="sm"
                onClick={() => publishMut.mutate()}
                disabled={publishMut.isPending}
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

      {/* Two-column editor + right inspector rail */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        {/* Form canvas */}
        <Card className="lg:col-span-8">
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

        {/* Right rail: Document info + Version History */}
        <div className="space-y-4 lg:col-span-4">
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
                  ))
                )}
              </div>
            </Card>
          )}
        </div>
      </div>
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
          <Input
            type="number"
            placeholder="Media asset ID (Media Library picker unlocks in Phase 4)"
            value={
              typeof value === "number"
                ? String(value)
                : typeof value === "object" && value !== null && "id" in value
                  ? String((value as { id: number }).id)
                  : ""
            }
            onChange={(e) => {
              const raw = e.target.value;
              onChange(raw === "" ? null : Number(raw));
            }}
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
