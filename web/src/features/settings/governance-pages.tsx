import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  CalendarClock,
  CheckCircle2,
  Play,
  Plus,
  Rocket,
  ShieldAlert,
  Star,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Checkbox,
  DataTable,
  Dialog,
  DialogContent,
  EmptyState,
  Input,
  Select,
  Switch,
  useToast,
} from "@/components/ui";
import { api, type AuditLogDto } from "@/lib/api";

// ============================================================================
// 1. Internationalization (i18n Locales) — `/settings/i18n`
// ============================================================================

const COMMON_LOCALE_PRESETS = [
  { code: "en", name: "English (en)" },
  { code: "fr", name: "French (fr)" },
  { code: "de", name: "German (de)" },
  { code: "es", name: "Spanish (es)" },
  { code: "bn", name: "Bengali (bn)" },
  { code: "ja", name: "Japanese (ja)" },
  { code: "ar", name: "Arabic (ar)" },
  { code: "zh-Hans", name: "Chinese Simplified (zh-Hans)" },
];

export function LocalesPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const [createOpen, setCreateOpen] = useState(false);
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [isDefault, setIsDefault] = useState(false);

  const localesQuery = useQuery({
    queryKey: ["locales"],
    queryFn: api.listLocales,
  });

  const createMut = useMutation({
    mutationFn: () => api.createLocale({ code: code.trim(), name: name.trim(), isDefault }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["locales"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      setCreateOpen(false);
      setCode("");
      setName("");
      setIsDefault(false);
      pushToast({ title: "Locale added", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Failed to add locale", description: e.message, tone: "error" }),
  });

  const setDefaultMut = useMutation({
    mutationFn: (id: number) => api.updateLocale(id, { isDefault: true }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["locales"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Updated default locale", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Update failed", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: number) => api.deleteLocale(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["locales"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Locale removed", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Cannot delete locale", description: e.message, tone: "error" }),
  });

  const locales = localesQuery.data ?? [];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            Internationalization (i18n)
          </h1>
          <p className="text-xs text-zinc-600">
            Configure BCP-47 locales for multi-language content variants and REST{" "}
            <code className="font-mono text-[11px]">?locale=</code> queries.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus className="h-3.5 w-3.5" />
          Add locale
        </Button>
      </div>

      <DataTable
        rows={locales}
        getRowKey={(r) => String(r.id)}
        emptyMessage="No locales configured."
        columns={[
          {
            key: "code",
            header: "ISO / BCP-47 Code",
            render: (r) => <Badge tone="mono">{r.code}</Badge>,
          },
          {
            key: "name",
            header: "Display Name",
            render: (r) => <span className="font-medium text-zinc-900">{r.name}</span>,
          },
          {
            key: "default",
            header: "Default",
            render: (r) =>
              r.isDefault ? (
                <Badge tone="emerald">
                  <Star className="h-3 w-3" />
                  Default
                </Badge>
              ) : (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setDefaultMut.mutate(r.id)}
                  disabled={setDefaultMut.isPending}
                >
                  Set default
                </Button>
              ),
          },
          {
            key: "createdAt",
            header: "Added",
            render: (r) => (
              <span className="font-mono text-[11px] tabular-nums text-zinc-500">
                {new Date(r.createdAt).toLocaleDateString()}
              </span>
            ),
          },
          {
            key: "actions",
            header: "",
            className: "text-right",
            render: (r) =>
              !r.isDefault && (
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={deleteMut.isPending}
                  onClick={() => {
                    if (window.confirm(`Delete locale '${r.code}'?`)) {
                      deleteMut.mutate(r.id);
                    }
                  }}
                >
                  <Trash2 className="h-3.5 w-3.5 text-rose-600" />
                </Button>
              ),
          },
        ]}
      />

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent title="Add i18n Locale">
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Quick preset</label>
              <Select
                value=""
                onValueChange={(v) => {
                  const found = COMMON_LOCALE_PRESETS.find((p) => p.code === v);
                  if (found) {
                    setCode(found.code);
                    setName(found.name);
                  }
                }}
                options={[
                  { value: "", label: "Choose common preset (optional)..." },
                  ...COMMON_LOCALE_PRESETS.map((p) => ({ value: p.code, label: p.name })),
                ]}
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">BCP-47 Code</label>
              <Input
                value={code}
                onChange={(e) => setCode(e.target.value)}
                placeholder="e.g. fr, de, bn, en-US"
                className="font-mono"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Display Name</label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="French (fr)"
              />
            </div>
            <div className="flex items-center justify-between rounded border border-zinc-200 bg-zinc-50 px-3 py-2">
              <span>Make this the default locale</span>
              <Switch checked={isDefault} onCheckedChange={setIsDefault} />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="secondary" size="sm" onClick={() => setCreateOpen(false)}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={!code.trim() || !name.trim() || createMut.isPending}
                onClick={() => createMut.mutate()}
              >
                Save locale
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 2. Preview URL Configuration — `/settings/preview`
// ============================================================================

export function PreviewSettingsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const schemasQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });
  const configsQuery = useQuery({
    queryKey: ["preview-configs"],
    queryFn: api.listPreviewConfigs,
  });

  const [editingUid, setEditingUid] = useState<string | null>(null);
  const [urlTemplate, setUrlTemplate] = useState("");
  const [openMode, setOpenMode] = useState<"split" | "tab">("split");
  const [isEnabled, setIsEnabled] = useState(true);

  const saveMut = useMutation({
    mutationFn: () =>
      api.upsertPreviewConfig(editingUid!, {
        urlTemplate,
        isEnabled,
        openMode,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["preview-configs"] });
      setEditingUid(null);
      pushToast({ title: "Saved preview configuration", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Save failed", description: e.message, tone: "error" }),
  });

  const resetMut = useMutation({
    mutationFn: (uid: string) => api.deletePreviewConfig(uid),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["preview-configs"] });
      pushToast({ title: "Reverted to built-in RAPI preview", tone: "success" });
    },
  });

  const contentTypes = schemasQuery.data?.contentTypes ?? [];
  const configMap = new Map((configsQuery.data ?? []).map((c) => [c.contentTypeUid, c]));

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
          Live Preview Configuration
        </h1>
        <p className="text-xs text-zinc-600">
          Configure external frontend preview URL templates or use RAPI's built-in live SSE preview
          panel (<code className="font-mono text-[11px]">/api/preview/events</code>). Supported
          tokens:{" "}
          <code className="font-mono text-[11px]">
            {"{pluralName} {singularName} {documentId} {locale} {status}"}
          </code>
          .
        </p>
      </div>

      <DataTable
        rows={contentTypes}
        getRowKey={(ct) => ct.uid}
        emptyMessage="No content types created yet."
        columns={[
          {
            key: "name",
            header: "Content Type",
            render: (ct) => (
              <div>
                <div className="font-medium text-zinc-900">{ct.displayName}</div>
                <div className="font-mono text-[11px] text-zinc-500">{ct.uid}</div>
              </div>
            ),
          },
          {
            key: "template",
            header: "Preview URL Template",
            render: (ct) => {
              const cfg = configMap.get(ct.uid);
              return cfg ? (
                <span className="font-mono text-[11px] text-zinc-800">{cfg.urlTemplate}</span>
              ) : (
                <Badge tone="neutral">Built-in Live Preview</Badge>
              );
            },
          },
          {
            key: "mode",
            header: "Mode",
            render: (ct) => {
              const cfg = configMap.get(ct.uid);
              return <Badge tone="mono">{cfg?.openMode ?? "split"}</Badge>;
            },
          },
          {
            key: "actions",
            header: "",
            className: "text-right",
            render: (ct) => {
              const cfg = configMap.get(ct.uid);
              return (
                <div className="inline-flex items-center gap-1.5">
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      setEditingUid(ct.uid);
                      setUrlTemplate(
                        cfg?.urlTemplate ??
                          `http://localhost:3000/api/preview?slug=${ct.pluralName}&documentId={documentId}&locale={locale}&status={status}`,
                      );
                      setOpenMode((cfg?.openMode as "split" | "tab") ?? "split");
                      setIsEnabled(cfg?.isEnabled ?? true);
                    }}
                  >
                    Configure
                  </Button>
                  {cfg && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => resetMut.mutate(ct.uid)}
                    >
                      Reset
                    </Button>
                  )}
                </div>
              );
            },
          },
        ]}
      />

      <Dialog open={Boolean(editingUid)} onOpenChange={(o) => !o && setEditingUid(null)}>
        <DialogContent title={`Configure Preview · ${editingUid ?? ""}`}>
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">URL Template</label>
              <Input
                value={urlTemplate}
                onChange={(e) => setUrlTemplate(e.target.value)}
                placeholder="https://site.example.com/preview/{pluralName}/{documentId}?locale={locale}&status={status}"
                className="font-mono text-[11px]"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Default Open Mode</label>
              <Select
                value={openMode}
                onValueChange={(v) => setOpenMode(v as "split" | "tab")}
                options={[
                  { value: "split", label: "Split pane inside Entry Editor" },
                  { value: "tab", label: "New browser tab" },
                ]}
              />
            </div>
            <div className="flex items-center justify-between rounded border border-zinc-200 bg-zinc-50 px-3 py-2">
              <span>Enable custom preview URL</span>
              <Switch checked={isEnabled} onCheckedChange={setIsEnabled} />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="secondary" size="sm" onClick={() => setEditingUid(null)}>
                Cancel
              </Button>
              <Button size="sm" onClick={() => saveMut.mutate()} disabled={saveMut.isPending}>
                Save template
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 3. Releases & Scheduled Publishing — `/settings/releases`
// ============================================================================

export function ReleasesPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const releasesQuery = useQuery({
    queryKey: ["releases"],
    queryFn: api.listReleases,
  });

  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [scheduledLocal, setScheduledLocal] = useState("");
  const [rescheduleId, setRescheduleId] = useState<string | null>(null);
  const [rescheduleLocal, setRescheduleLocal] = useState("");

  const createMut = useMutation({
    mutationFn: () => {
      const scheduledAt = scheduledLocal ? new Date(scheduledLocal).toISOString() : null;
      return api.createRelease({
        name: name.trim(),
        scheduledAt,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["releases"] });
      setCreateOpen(false);
      setName("");
      setScheduledLocal("");
      pushToast({ title: "Release group created", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Create failed", description: e.message, tone: "error" }),
  });

  const rescheduleMut = useMutation({
    mutationFn: () => {
      const scheduledAt = rescheduleLocal ? new Date(rescheduleLocal).toISOString() : null;
      return api.updateRelease(rescheduleId!, {
        scheduledAt,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["releases"] });
      setRescheduleId(null);
      setRescheduleLocal("");
      pushToast({ title: "Release schedule updated", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Reschedule failed", description: e.message, tone: "error" }),
  });

  const publishReleaseMut = useMutation({
    mutationFn: (id: string) => api.publishRelease(id),
    onSuccess: (rel) => {
      queryClient.invalidateQueries({ queryKey: ["releases"] });
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({
        title: `Published release "${rel.name}" (${rel.entryCount} entries)`,
        tone: "success",
      });
    },
    onError: (e: Error) => {
      queryClient.invalidateQueries({ queryKey: ["releases"] });
      pushToast({ title: "Release failed", description: e.message, tone: "error" });
    },
  });

  const removeEntryMut = useMutation({
    mutationFn: ({ releaseId, entryId }: { releaseId: string; entryId: number }) =>
      api.removeReleaseEntry(releaseId, entryId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["releases"] }),
  });

  const deleteReleaseMut = useMutation({
    mutationFn: (id: string) => api.deleteRelease(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["releases"] });
      pushToast({ title: "Release deleted", tone: "success" });
    },
  });

  const releases = releasesQuery.data ?? [];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">Releases</h1>
          <p className="text-xs text-zinc-600">
            Group multi-document publish/unpublish actions into atomic releases or schedule them
            for automatic execution by the background worker.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus className="h-3.5 w-3.5" />
          New release
        </Button>
      </div>

      {releases.length === 0 ? (
        <EmptyState
          icon={Rocket}
          title="No releases created"
          description="Create a release here and add draft entries from the Entry Editor inspector rail."
          action={
            <Button size="sm" onClick={() => setCreateOpen(true)}>
              Create first release
            </Button>
          }
        />
      ) : (
        <div className="space-y-4">
          {releases.map((rel) => (
            <Card key={rel.id}>
              <CardHeader className="flex flex-row items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-2">
                  <CardTitle>{rel.name}</CardTitle>
                  <Badge
                    tone={
                      rel.status === "done"
                        ? "emerald"
                        : rel.status === "failed"
                          ? "rose"
                          : "amber"
                    }
                  >
                    {rel.status}
                  </Badge>
                  <Badge tone="mono">{rel.entryCount} entries</Badge>
                  {rel.scheduledAt && (
                    <Badge tone="neutral">
                      <CalendarClock className="h-3 w-3" />
                      {new Date(rel.scheduledAt).toLocaleString()}
                    </Badge>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  {rel.status !== "done" && (
                    <>
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                          setRescheduleId(rel.id);
                          setRescheduleLocal(
                            rel.scheduledAt ? rel.scheduledAt.slice(0, 16) : "",
                          );
                        }}
                      >
                        <CalendarClock className="h-3.5 w-3.5" />
                        Schedule
                      </Button>
                      <Button
                        size="sm"
                        disabled={rel.entryCount === 0 || publishReleaseMut.isPending}
                        onClick={() => publishReleaseMut.mutate(rel.id)}
                      >
                        <Play className="h-3.5 w-3.5" />
                        Publish now
                      </Button>
                    </>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      if (window.confirm(`Delete release "${rel.name}"?`)) {
                        deleteReleaseMut.mutate(rel.id);
                      }
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5 text-rose-600" />
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-2 text-xs">
                {rel.errorMessage && (
                  <div className="rounded border border-rose-200 bg-rose-50 px-3 py-2 text-rose-800">
                    {rel.errorMessage}
                  </div>
                )}
                {rel.entries.length === 0 ? (
                  <p className="text-zinc-500">
                    No entries attached yet. Open any document in{" "}
                    <Link to="/content-manager" className="text-indigo-700 underline">
                      Content Manager
                    </Link>{" "}
                    and use "Add to Release".
                  </p>
                ) : (
                  <div className="divide-y divide-zinc-200 rounded border border-zinc-200">
                    {rel.entries.map((entry) => (
                      <div
                        key={entry.id}
                        className="flex items-center justify-between px-3 py-2"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge tone={entry.action === "publish" ? "emerald" : "amber"}>
                            {entry.action}
                          </Badge>
                          <span className="font-mono text-[11px] text-zinc-700">
                            {entry.contentTypeUid}
                          </span>
                          <Link
                            to={`/content-manager/${encodeURIComponent(entry.contentTypeUid)}/${encodeURIComponent(entry.documentId)}`}
                            className="font-mono text-[11px] font-medium text-indigo-700 hover:underline"
                          >
                            {entry.documentId}
                          </Link>
                          <Badge tone="mono">{entry.locale}</Badge>
                        </div>
                        {rel.status !== "done" && (
                          <button
                            type="button"
                            onClick={() =>
                              removeEntryMut.mutate({ releaseId: rel.id, entryId: entry.id })
                            }
                            className="text-zinc-400 hover:text-rose-600 cursor-pointer"
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={Boolean(rescheduleId)} onOpenChange={(o) => !o && setRescheduleId(null)}>
        <DialogContent title="Update Release Schedule">
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">
                Scheduled Publish Time (leave blank for manual)
              </label>
              <Input
                type="datetime-local"
                value={rescheduleLocal}
                onChange={(e) => setRescheduleLocal(e.target.value)}
              />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="secondary" size="sm" onClick={() => setRescheduleId(null)}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={rescheduleMut.isPending}
                onClick={() => rescheduleMut.mutate()}
              >
                Save schedule
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent title="Create Release Group">
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Release Name</label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Spring Launch Campaign"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">
                Scheduled Publish Time (optional)
              </label>
              <Input
                type="datetime-local"
                value={scheduledLocal}
                onChange={(e) => setScheduledLocal(e.target.value)}
              />
              <p className="mt-1 text-[11px] text-zinc-500">
                Leave empty for manual execution, or set a timestamp for the background worker.
              </p>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="secondary" size="sm" onClick={() => setCreateOpen(false)}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={!name.trim() || createMut.isPending}
                onClick={() => createMut.mutate()}
              >
                Create release
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 4. Review Workflows & Stage Gates — `/settings/workflows`
// ============================================================================

export function WorkflowsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const stagesQuery = useQuery({
    queryKey: ["workflow-stages"],
    queryFn: api.listWorkflowStages,
  });
  const rolesQuery = useQuery({
    queryKey: ["admin-roles"],
    queryFn: api.listRoles,
  });

  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [color, setColor] = useState("#4f46e5");
  const [requireForPublish, setRequireForPublish] = useState(false);
  const [selectedRoles, setSelectedRoles] = useState<string[]>([]);

  const createMut = useMutation({
    mutationFn: () =>
      api.createWorkflowStage({
        name: name.trim(),
        color,
        allowedRoleIds: selectedRoles,
        requireForPublish,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workflow-stages"] });
      setCreateOpen(false);
      setName("");
      setSelectedRoles([]);
      setRequireForPublish(false);
      pushToast({ title: "Workflow stage created", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Failed to create stage", description: e.message, tone: "error" }),
  });

  const togglePublishGateMut = useMutation({
    mutationFn: ({ id, next }: { id: string; next: boolean }) =>
      api.updateWorkflowStage(id, { requireForPublish: next }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workflow-stages"] });
      pushToast({ title: "Updated workflow publish gate", tone: "success" });
    },
  });

  const reorderStageMut = useMutation({
    mutationFn: async ({ idx, dir }: { idx: number; dir: -1 | 1 }) => {
      const targetIdx = idx + dir;
      const a = stages[idx];
      const b = stages[targetIdx];
      if (!a || !b) return;
      await Promise.all([
        api.updateWorkflowStage(a.id, { position: b.position }),
        api.updateWorkflowStage(b.id, { position: a.position }),
      ]);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workflow-stages"] });
    },
  });

  const deleteStageMut = useMutation({
    mutationFn: (id: string) => api.deleteWorkflowStage(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workflow-stages"] });
      pushToast({ title: "Stage deleted", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Cannot delete stage", description: e.message, tone: "error" }),
  });

  const stages = stagesQuery.data ?? [];
  const roles = rolesQuery.data ?? [];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            Review Workflows
          </h1>
          <p className="text-xs text-zinc-600">
            Customize editorial stages, role transition permissions, and mandatory publish-approval
            gates.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus className="h-3.5 w-3.5" />
          Add stage
        </Button>
      </div>

      <DataTable
        rows={stages}
        getRowKey={(s) => s.id}
        emptyMessage="No workflow stages configured."
        columns={[
          {
            key: "pos",
            header: "#",
            render: (s) => <span className="font-mono text-xs tabular-nums">{s.position + 1}</span>,
          },
          {
            key: "stage",
            header: "Stage",
            render: (s) => (
              <div className="flex items-center gap-2">
                <span
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ backgroundColor: s.color }}
                />
                <span className="font-medium text-zinc-900">{s.name}</span>
              </div>
            ),
          },
          {
            key: "roles",
            header: "Role Gate",
            render: (s) =>
              s.allowedRoleIds.length === 0 ? (
                <Badge tone="neutral">All admin roles</Badge>
              ) : (
                <Badge tone="mono">{s.allowedRoleIds.length} role(s)</Badge>
              ),
          },
          {
            key: "gate",
            header: "Required for Publish",
            render: (s) => (
              <div className="flex items-center gap-2">
                <Switch
                  checked={s.requireForPublish}
                  onCheckedChange={(next) =>
                    togglePublishGateMut.mutate({ id: s.id, next })
                  }
                />
                {s.requireForPublish && (
                  <Badge tone="emerald">
                    <CheckCircle2 className="h-3 w-3" />
                    Publish gate
                  </Badge>
                )}
              </div>
            ),
          },
          {
            key: "actions",
            header: "",
            className: "text-right",
            render: (s) => {
              const idx = stages.findIndex((x) => x.id === s.id);
              return (
                <div className="inline-flex items-center gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    title="Move stage up"
                    disabled={idx <= 0 || reorderStageMut.isPending}
                    onClick={() => reorderStageMut.mutate({ idx, dir: -1 })}
                  >
                    <ArrowUp className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    title="Move stage down"
                    disabled={idx < 0 || idx >= stages.length - 1 || reorderStageMut.isPending}
                    onClick={() => reorderStageMut.mutate({ idx, dir: 1 })}
                  >
                    <ArrowDown className="h-3.5 w-3.5" />
                  </Button>
                  {stages.length > 1 && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        if (window.confirm(`Delete stage "${s.name}"?`)) {
                          deleteStageMut.mutate(s.id);
                        }
                      }}
                    >
                      <Trash2 className="h-3.5 w-3.5 text-rose-600" />
                    </Button>
                  )}
                </div>
              );
            },
          },
        ]}
      />

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent title="Add Review Workflow Stage">
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Stage Name</label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Legal Sign-Off"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Badge Color</label>
              <div className="flex items-center gap-2">
                <input
                  type="color"
                  value={color}
                  onChange={(e) => setColor(e.target.value)}
                  className="h-8 w-10 cursor-pointer rounded border border-zinc-300"
                />
                <Input
                  value={color}
                  onChange={(e) => setColor(e.target.value)}
                  className="font-mono"
                />
              </div>
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">
                Restrict Stage Transition to Roles (empty = all roles)
              </label>
              <div className="space-y-1.5 rounded border border-zinc-200 p-2.5">
                {roles.map((r) => {
                  const checked = selectedRoles.includes(r.id);
                  return (
                    <label key={r.id} className="flex items-center gap-2 cursor-pointer">
                      <Checkbox
                        checked={checked}
                        onCheckedChange={(next) =>
                          setSelectedRoles((prev) =>
                            next ? [...prev, r.id] : prev.filter((x) => x !== r.id),
                          )
                        }
                      />
                      <span>{r.name}</span>
                      <span className="font-mono text-[10px] text-zinc-400">({r.code})</span>
                    </label>
                  );
                })}
              </div>
            </div>
            <div className="flex items-center justify-between rounded border border-zinc-200 bg-zinc-50 px-3 py-2">
              <span>Require this stage before entries can be published</span>
              <Switch checked={requireForPublish} onCheckedChange={setRequireForPublish} />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="secondary" size="sm" onClick={() => setCreateOpen(false)}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={!name.trim() || createMut.isPending}
                onClick={() => createMut.mutate()}
              >
                Create stage
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 5. Audit Logs — `/settings/audit-logs`
// ============================================================================

export function AuditLogsPage() {
  const [actionFilter, setActionFilter] = useState("");
  const [actorFilter, setActorFilter] = useState("");
  const [page, setPage] = useState(1);
  const [selectedLog, setSelectedLog] = useState<AuditLogDto | null>(null);

  const logsQuery = useQuery({
    queryKey: ["audit-logs", actionFilter, actorFilter, page],
    queryFn: () =>
      api.listAuditLogs({
        action: actionFilter.trim() || undefined,
        actorId: actorFilter.trim() || undefined,
        page,
        pageSize: 25,
      }),
  });

  const rows = logsQuery.data?.data ?? [];
  const pagination = logsQuery.data?.meta.pagination;

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-900">Audit Logs</h1>
        <p className="text-xs text-zinc-600">
          Immutable trail of administrator mutations across schemas, documents, media, roles,
          tokens, releases, and workflows (credentials automatically redacted).
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2.5 rounded-md border border-zinc-200 bg-white p-3">
        <Input
          value={actionFilter}
          onChange={(e) => {
            setActionFilter(e.target.value);
            setPage(1);
          }}
          placeholder="Filter action (e.g. entry.publish, schema.create)..."
          className="max-w-xs font-mono text-xs"
        />
        <Input
          value={actorFilter}
          onChange={(e) => {
            setActorFilter(e.target.value);
            setPage(1);
          }}
          placeholder="Filter actor email or token..."
          className="max-w-xs font-mono text-xs"
        />
      </div>

      <DataTable
        rows={rows}
        getRowKey={(r) => r.id}
        emptyMessage="No matching audit events."
        columns={[
          {
            key: "time",
            header: "Timestamp",
            render: (r) => (
              <span className="font-mono text-[11px] tabular-nums text-zinc-600">
                {new Date(r.createdAt).toLocaleString()}
              </span>
            ),
          },
          {
            key: "action",
            header: "Action",
            render: (r) => <Badge tone="mono">{r.action}</Badge>,
          },
          {
            key: "actor",
            header: "Actor",
            render: (r) => (
              <div className="text-xs">
                <div className="font-medium text-zinc-900">{r.actorId ?? "anonymous"}</div>
                <div className="font-mono text-[10px] text-zinc-400">{r.actorType}</div>
              </div>
            ),
          },
          {
            key: "target",
            header: "Target",
            render: (r) => (
              <span className="font-mono text-[11px] text-zinc-700">
                {[r.targetUid, r.targetId].filter(Boolean).join(" / ") || "—"}
              </span>
            ),
          },
          {
            key: "status",
            header: "HTTP",
            render: (r) => {
              const st = Number((r.payload as { status?: number })?.status ?? 200);
              return (
                <Badge tone={st >= 400 ? "amber" : "emerald"}>
                  {st}
                </Badge>
              );
            },
          },
          {
            key: "inspect",
            header: "",
            className: "text-right",
            render: (r) => (
              <Button size="sm" variant="ghost" onClick={() => setSelectedLog(r)}>
                <ShieldAlert className="h-3.5 w-3.5" />
                Payload
              </Button>
            ),
          },
        ]}
      />

      {pagination && pagination.pageCount > 1 && (
        <div className="flex items-center justify-between text-xs text-zinc-600">
          <span className="tabular-nums">
            Page {pagination.page} of {pagination.pageCount} ({pagination.total} events)
          </span>
          <div className="flex gap-2">
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

      <Dialog open={Boolean(selectedLog)} onOpenChange={(o) => !o && setSelectedLog(null)}>
        <DialogContent title={`Audit Event · ${selectedLog?.action ?? ""}`}>
          {selectedLog && (
            <div className="space-y-3 text-xs">
              <div className="grid grid-cols-2 gap-2 rounded border border-zinc-200 bg-zinc-50 p-2.5">
                <div>
                  <span className="text-zinc-500">Event ID:</span>{" "}
                  <span className="font-mono">{selectedLog.id}</span>
                </div>
                <div>
                  <span className="text-zinc-500">IP:</span>{" "}
                  <span className="font-mono">{selectedLog.ipAddress ?? "local"}</span>
                </div>
              </div>
              <pre className="max-h-80 overflow-auto rounded-md bg-zinc-950 p-3 font-mono text-[11px] text-zinc-100">
                {JSON.stringify(selectedLog.payload, null, 2)}
              </pre>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
