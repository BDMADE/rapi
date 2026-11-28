import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  Download,
  FileSpreadsheet,
  KeyRound,
  Mail,
  Play,
  Plus,
  Send,
  Trash2,
  Upload,
} from "lucide-react";
import { useState } from "react";
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
  Input,
  Select,
  Switch,
  useToast,
} from "@/components/ui";
import { api, type EmailTemplateDto, type WebhookDto } from "@/lib/api";

// ============================================================================
// 1. Webhooks + HMAC Signing + Delivery Logs — `/settings/webhooks`
// ============================================================================

const WEBHOOK_EVENTS = [
  "entry.create",
  "entry.update",
  "entry.delete",
  "entry.publish",
  "entry.unpublish",
  "media.create",
  "media.delete",
  "release.publish",
];

export function WebhooksPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const webhooksQuery = useQuery({
    queryKey: ["webhooks"],
    queryFn: api.listWebhooks,
  });

  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [secret, setSecret] = useState("");
  const [selectedEvents, setSelectedEvents] = useState<string[]>([
    "entry.create",
    "entry.update",
    "entry.publish",
  ]);
  const [inspectWebhook, setInspectWebhook] = useState<WebhookDto | null>(null);

  const deliveriesQuery = useQuery({
    queryKey: ["webhook-deliveries", inspectWebhook?.id],
    queryFn: () => api.listWebhookDeliveries(inspectWebhook!.id),
    enabled: Boolean(inspectWebhook),
  });

  const createMut = useMutation({
    mutationFn: () =>
      api.createWebhook({
        name: name.trim(),
        url: url.trim(),
        events: selectedEvents,
        secret: secret.trim() || undefined,
        isEnabled: true,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["webhooks"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      setCreateOpen(false);
      setName("");
      setUrl("");
      setSecret("");
      pushToast({ title: "Webhook created", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Webhook error", description: e.message, tone: "error" }),
  });

  const toggleEnabledMut = useMutation({
    mutationFn: ({ id, isEnabled }: { id: string; isEnabled: boolean }) =>
      api.updateWebhook(id, { isEnabled }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["webhooks"] }),
  });

  const triggerMut = useMutation({
    mutationFn: (id: string) => api.triggerWebhook(id),
    onSuccess: (delivery) => {
      queryClient.invalidateQueries({ queryKey: ["webhook-deliveries"] });
      pushToast({
        title: delivery.success
          ? `Test webhook delivered (HTTP ${delivery.statusCode})`
          : `Webhook failed (${delivery.responseBody ?? "unreachable"})`,
        tone: delivery.success ? "success" : "error",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "Trigger failed", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => api.deleteWebhook(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["webhooks"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Webhook deleted", tone: "success" });
    },
  });

  const webhooks = webhooksQuery.data ?? [];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">Webhooks</h1>
          <p className="text-xs text-zinc-600">
            Deliver HMAC-SHA256 signed event notifications (<code className="font-mono text-[11px]">X-Rapi-Signature</code>) with background worker retries.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus className="h-3.5 w-3.5" />
          Create webhook
        </Button>
      </div>

      <DataTable
        rows={webhooks}
        getRowKey={(w) => w.id}
        emptyMessage="No webhooks configured yet."
        columns={[
          {
            key: "name",
            header: "Name & Endpoint",
            render: (w) => (
              <div>
                <div className="flex items-center gap-1.5 font-medium text-zinc-900">
                  <span>{w.name}</span>
                  {w.hasSecret && (
                    <Badge tone="emerald">
                      <KeyRound className="h-3 w-3" />
                      HMAC
                    </Badge>
                  )}
                </div>
                <div className="font-mono text-[11px] text-zinc-500">{w.url}</div>
              </div>
            ),
          },
          {
            key: "events",
            header: "Subscribed Events",
            render: (w) => (
              <div className="flex flex-wrap gap-1">
                {w.events.slice(0, 3).map((ev) => (
                  <Badge key={ev} tone="mono">
                    {ev}
                  </Badge>
                ))}
                {w.events.length > 3 && (
                  <Badge tone="neutral">+{w.events.length - 3}</Badge>
                )}
              </div>
            ),
          },
          {
            key: "enabled",
            header: "Enabled",
            render: (w) => (
              <Switch
                checked={w.isEnabled}
                onCheckedChange={(next) =>
                  toggleEnabledMut.mutate({ id: w.id, isEnabled: next })
                }
              />
            ),
          },
          {
            key: "actions",
            header: "",
            className: "text-right",
            render: (w) => (
              <div className="inline-flex items-center gap-1.5">
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={triggerMut.isPending}
                  onClick={() => triggerMut.mutate(w.id)}
                >
                  <Play className="h-3 w-3" />
                  Trigger
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setInspectWebhook(w)}
                >
                  <Activity className="h-3.5 w-3.5" />
                  Logs
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    if (window.confirm(`Delete webhook "${w.name}"?`)) {
                      deleteMut.mutate(w.id);
                    }
                  }}
                >
                  <Trash2 className="h-3.5 w-3.5 text-rose-600" />
                </Button>
              </div>
            ),
          },
        ]}
      />

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent title="Create Signed Webhook">
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Name</label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Vercel Rebuild Hook"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Target URL</label>
              <Input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://api.example.com/webhooks/rapi"
                className="font-mono text-[11px]"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">
                HMAC-SHA256 Signing Secret (optional)
              </label>
              <Input
                type="password"
                value={secret}
                onChange={(e) => setSecret(e.target.value)}
                placeholder="whsec_..."
                className="font-mono text-[11px]"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Events</label>
              <div className="grid grid-cols-2 gap-1.5 rounded border border-zinc-200 p-2.5">
                {WEBHOOK_EVENTS.map((ev) => {
                  const checked = selectedEvents.includes(ev);
                  return (
                    <label key={ev} className="flex cursor-pointer items-center gap-2">
                      <Checkbox
                        checked={checked}
                        onCheckedChange={(val) =>
                          setSelectedEvents((prev) =>
                            val ? [...prev, ev] : prev.filter((x) => x !== ev),
                          )
                        }
                      />
                      <span className="font-mono text-[11px]">{ev}</span>
                    </label>
                  );
                })}
              </div>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button size="sm" variant="secondary" onClick={() => setCreateOpen(false)}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={
                  !name.trim() ||
                  !url.trim() ||
                  selectedEvents.length === 0 ||
                  createMut.isPending
                }
                onClick={() => createMut.mutate()}
              >
                Save webhook
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(inspectWebhook)}
        onOpenChange={(o) => !o && setInspectWebhook(null)}
      >
        <DialogContent
          title={`Delivery Logs · ${inspectWebhook?.name ?? ""}`}
          className="max-w-2xl"
        >
          <div className="max-h-96 space-y-2 overflow-y-auto text-xs">
            {(deliveriesQuery.data ?? []).length === 0 ? (
              <p className="py-6 text-center text-zinc-500">
                No deliveries recorded yet. Click "Trigger" to send a test ping.
              </p>
            ) : (
              (deliveriesQuery.data ?? []).map((d) => (
                <div
                  key={d.id}
                  className="space-y-1.5 rounded border border-zinc-200 bg-zinc-50 p-2.5"
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Badge tone={d.success ? "emerald" : "rose"}>
                        {d.statusCode ? `HTTP ${d.statusCode}` : "ERR"}
                      </Badge>
                      <Badge tone="mono">{d.event}</Badge>
                      <span className="font-mono text-[10px] text-zinc-500">
                        {d.durationMs ?? 0}ms · attempt #{d.attempts}
                      </span>
                    </div>
                    <span className="font-mono text-[10px] text-zinc-500">
                      {new Date(d.createdAt).toLocaleString()}
                    </span>
                  </div>
                  {d.responseBody && (
                    <pre className="max-h-24 overflow-auto rounded bg-zinc-950 p-2 font-mono text-[10px] text-zinc-200">
                      {d.responseBody}
                    </pre>
                  )}
                </div>
              ))
            )}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 2. Email Plugin (`lettre` SMTP + Templates + Outbox) — `/settings/email`
// ============================================================================

export function EmailSettingsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const statusQuery = useQuery({
    queryKey: ["email-settings"],
    queryFn: api.getEmailSettings,
  });
  const templatesQuery = useQuery({
    queryKey: ["email-templates"],
    queryFn: api.listEmailTemplates,
  });
  const outboxQuery = useQuery({
    queryKey: ["email-outbox"],
    queryFn: api.listEmailOutbox,
  });

  const [testTo, setTestTo] = useState("");
  const [editingTpl, setEditingTpl] = useState<EmailTemplateDto | null>(null);
  const [tplSubject, setTplSubject] = useState("");
  const [tplHtml, setTplHtml] = useState("");
  const [tplText, setTplText] = useState("");

  const sendTestMut = useMutation({
    mutationFn: () => api.sendTestEmail(testTo.trim()),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["email-outbox"] });
      setTestTo("");
      pushToast({ title: "Test email dispatched", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Send failed", description: e.message, tone: "error" }),
  });

  const saveTplMut = useMutation({
    mutationFn: () =>
      api.upsertEmailTemplate(editingTpl!.code, {
        subject: tplSubject,
        htmlBody: tplHtml,
        textBody: tplText,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["email-templates"] });
      setEditingTpl(null);
      pushToast({ title: "Email template saved", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Save failed", description: e.message, tone: "error" }),
  });

  const st = statusQuery.data;

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
          Email Delivery &amp; Templates
        </h1>
        <p className="text-xs text-zinc-600">
          Powered by <code className="font-mono text-[11px]">lettre</code> SMTP when{" "}
          <code className="font-mono text-[11px]">SMTP_HOST</code> is set, with zero-config SQLite
          outbox capture in local development.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Transport Status</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-xs">
            <div className="flex items-center justify-between">
              <span className="text-zinc-500">Active Provider</span>
              <Badge tone={st?.smtpConfigured ? "emerald" : "neutral"}>
                {st?.provider ?? "loading"}
              </Badge>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-zinc-500">Default Sender</span>
              <span className="font-mono">{st?.fromAddress ?? "noreply@rapi.local"}</span>
            </div>
            {st?.smtpHost && (
              <div className="flex items-center justify-between">
                <span className="text-zinc-500">SMTP Relay</span>
                <span className="font-mono">
                  {st.smtpHost}:{st.smtpPort}
                </span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Send Test Email</CardTitle>
          </CardHeader>
          <CardContent className="flex items-center gap-2">
            <Input
              type="email"
              value={testTo}
              onChange={(e) => setTestTo(e.target.value)}
              placeholder="developer@example.com"
            />
            <Button
              size="sm"
              disabled={!testTo.trim() || sendTestMut.isPending}
              onClick={() => sendTestMut.mutate()}
            >
              <Send className="h-3.5 w-3.5" />
              Send
            </Button>
          </CardContent>
        </Card>
      </div>

      <div className="space-y-2">
        <h2 className="text-sm font-semibold text-zinc-900">Transactional Templates</h2>
        <DataTable
          rows={templatesQuery.data ?? []}
          getRowKey={(t) => t.id}
          emptyMessage="No email templates configured."
          columns={[
            {
              key: "code",
              header: "Code",
              render: (t) => <Badge tone="mono">{t.code}</Badge>,
            },
            {
              key: "subject",
              header: "Subject",
              render: (t) => <span className="font-medium text-zinc-900">{t.subject}</span>,
            },
            {
              key: "edit",
              header: "",
              className: "text-right",
              render: (t) => (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    setEditingTpl(t);
                    setTplSubject(t.subject);
                    setTplHtml(t.htmlBody);
                    setTplText(t.textBody);
                  }}
                >
                  <Mail className="h-3.5 w-3.5" />
                  Edit template
                </Button>
              ),
            },
          ]}
        />
      </div>

      <div className="space-y-2">
        <h2 className="text-sm font-semibold text-zinc-900">Recent Outbox Deliveries</h2>
        <DataTable
          rows={outboxQuery.data ?? []}
          getRowKey={(o) => o.id}
          emptyMessage="No emails sent yet."
          columns={[
            {
              key: "to",
              header: "Recipient",
              render: (o) => <span className="font-mono text-xs">{o.toAddress}</span>,
            },
            {
              key: "subject",
              header: "Subject",
              render: (o) => <span>{o.subject}</span>,
            },
            {
              key: "transport",
              header: "Transport",
              render: (o) => <Badge tone="mono">{o.transport}</Badge>,
            },
            {
              key: "status",
              header: "Status",
              render: (o) => (
                <Badge tone={o.status === "sent" ? "emerald" : "rose"}>{o.status}</Badge>
              ),
            },
            {
              key: "time",
              header: "Sent",
              render: (o) => (
                <span className="font-mono text-[11px] tabular-nums text-zinc-500">
                  {new Date(o.createdAt).toLocaleString()}
                </span>
              ),
            },
          ]}
        />
      </div>

      <Dialog open={Boolean(editingTpl)} onOpenChange={(o) => !o && setEditingTpl(null)}>
        <DialogContent title={`Edit Email Template · ${editingTpl?.code ?? ""}`}>
          <div className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Subject</label>
              <Input value={tplSubject} onChange={(e) => setTplSubject(e.target.value)} />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">HTML Body</label>
              <textarea
                rows={5}
                value={tplHtml}
                onChange={(e) => setTplHtml(e.target.value)}
                className="w-full rounded border border-zinc-300 bg-zinc-950 p-2 font-mono text-[11px] text-zinc-100"
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Plaintext Fallback</label>
              <textarea
                rows={3}
                value={tplText}
                onChange={(e) => setTplText(e.target.value)}
                className="w-full rounded border border-zinc-300 p-2 font-mono text-[11px]"
              />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button size="sm" variant="secondary" onClick={() => setEditingTpl(null)}>
                Cancel
              </Button>
              <Button size="sm" disabled={saveTplMut.isPending} onClick={() => saveTplMut.mutate()}>
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
// 3. Data Management / Transfer (JSON, CSV, Strapi v5) — `/settings/data-transfer`
// ============================================================================

export function DataTransferPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const schemasQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });

  const contentTypes = schemasQuery.data?.contentTypes ?? [];
  const [selectedUid, setSelectedUid] = useState("");
  const [strapiJsonText, setStrapiJsonText] = useState("");

  const effectiveUid = selectedUid || contentTypes[0]?.uid || "";

  const handleDownloadJson = async () => {
    try {
      const bundle = await api.exportJsonBundle();
      const blob = new Blob([JSON.stringify(bundle, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `rapi-bundle-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      pushToast({ title: "Downloaded JSON platform bundle", tone: "success" });
    } catch (e) {
      pushToast({
        title: "Export failed",
        description: e instanceof Error ? e.message : "Unknown error",
        tone: "error",
      });
    }
  };

  const handleDownloadCsv = async () => {
    if (!effectiveUid) return;
    try {
      const csv = await api.exportCsvText(effectiveUid);
      const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${effectiveUid.replace(/[^a-z0-9]/gi, "_")}.csv`;
      a.click();
      URL.revokeObjectURL(url);
      pushToast({ title: "Downloaded CSV export", tone: "success" });
    } catch (e) {
      pushToast({
        title: "CSV export failed",
        description: e instanceof Error ? e.message : "Unknown error",
        tone: "error",
      });
    }
  };

  const importJsonMut = useMutation({
    mutationFn: (bundle: unknown) => api.importJsonBundle(bundle),
    onSuccess: (res) => {
      queryClient.invalidateQueries();
      pushToast({
        title: `Imported ${res.schemasImported} schemas & ${res.documentsImported} documents`,
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "JSON import failed", description: e.message, tone: "error" }),
  });

  const importCsvMut = useMutation({
    mutationFn: (csvText: string) => api.importCsv(effectiveUid, csvText),
    onSuccess: (res) => {
      queryClient.invalidateQueries({ queryKey: ["cm-entries"] });
      pushToast({
        title: `Imported ${res.documentsImported} CSV rows into ${effectiveUid}`,
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "CSV import failed", description: e.message, tone: "error" }),
  });

  const importStrapiMut = useMutation({
    mutationFn: () => api.importStrapiV5Archive(JSON.parse(strapiJsonText)),
    onSuccess: (res) => {
      queryClient.invalidateQueries();
      setStrapiJsonText("");
      pushToast({
        title: `Migrated ${res.schemasImported} Strapi v5 schemas & ${res.documentsImported} entries`,
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "Strapi v5 import failed", description: e.message, tone: "error" }),
  });

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
          Data Management &amp; Strapi v5 Migration
        </h1>
        <p className="text-xs text-zinc-600">
          Export or restore full JSON snapshots, per-collection RFC-4180 CSV tables, or migrate
          existing Strapi v5 JSON archives directly into RAPI.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Full Platform JSON Snapshot</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <p className="text-zinc-600">
              Includes all i18n locales, runtime content-type &amp; component schemas, and draft +
              published documents.
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" onClick={handleDownloadJson}>
                <Download className="h-3.5 w-3.5" />
                Export JSON bundle
              </Button>
              <label className="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50">
                <Upload className="h-3.5 w-3.5" />
                Import JSON file
                <input
                  type="file"
                  accept=".json,application/json"
                  className="hidden"
                  onChange={async (e) => {
                    const f = e.target.files?.[0];
                    if (!f) return;
                    const txt = await f.text();
                    e.target.value = "";
                    try {
                      importJsonMut.mutate(JSON.parse(txt));
                    } catch {
                      pushToast({ title: "Invalid JSON file", tone: "error" });
                    }
                  }}
                />
              </label>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Collection CSV Export / Import</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <Select
              value={effectiveUid}
              onValueChange={setSelectedUid}
              options={
                contentTypes.length > 0
                  ? contentTypes.map((ct) => ({
                      value: ct.uid,
                      label: `${ct.displayName} (${ct.uid})`,
                    }))
                  : [{ value: "", label: "No content types available" }]
              }
            />
            <div className="flex flex-wrap items-center gap-2">
              <Button
                size="sm"
                variant="secondary"
                disabled={!effectiveUid}
                onClick={handleDownloadCsv}
              >
                <FileSpreadsheet className="h-3.5 w-3.5" />
                Export CSV
              </Button>
              {effectiveUid && (
                <label className="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50">
                  <Upload className="h-3.5 w-3.5" />
                  Import CSV
                  <input
                    type="file"
                    accept=".csv,text/csv"
                    className="hidden"
                    onChange={async (e) => {
                      const f = e.target.files?.[0];
                      if (!f) return;
                      const txt = await f.text();
                      e.target.value = "";
                      importCsvMut.mutate(txt);
                    }}
                  />
                </label>
              )}
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Strapi v5 Archive Importer</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-xs">
          <p className="text-zinc-600">
            Paste a Strapi v5 export JSON object (<code className="font-mono text-[11px]">{"{ schemas: [...], entities: [...] }"}</code>) to provision SQLite tables and import entries automatically.
          </p>
          <textarea
            rows={6}
            value={strapiJsonText}
            onChange={(e) => setStrapiJsonText(e.target.value)}
            placeholder='{"schemas": [{"uid": "api::article.article", "kind": "collectionType", "info": {"singularName": "article", "pluralName": "articles", "displayName": "Article"}, "attributes": {"title": {"type": "string", "required": true}}}], "entities": [{"type": "api::article.article", "data": {"title": "Migrated from Strapi v5", "publishedAt": "2026-01-01T00:00:00Z"}}]}'
            className="w-full rounded-md border border-zinc-300 bg-zinc-950 p-3 font-mono text-[11px] text-zinc-100"
          />
          <div className="flex justify-end">
            <Button
              size="sm"
              disabled={!strapiJsonText.trim() || importStrapiMut.isPending}
              onClick={() => {
                try {
                  JSON.parse(strapiJsonText);
                  importStrapiMut.mutate();
                } catch {
                  pushToast({ title: "Invalid JSON syntax", tone: "error" });
                }
              }}
            >
              <Upload className="h-3.5 w-3.5" />
              Import Strapi v5 Archive
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
