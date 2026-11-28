import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Bot,
  Check,
  Copy,
  Cpu,
  Download,
  KeyRound,
  Layers,
  Play,
  Plug,
  Plus,
  ShieldCheck,
  Sparkles,
  Terminal,
  Trash2,
  Upload,
} from "lucide-react";
import { type ChangeEvent, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  DataTable,
  Dialog,
  DialogContent,
  Input,
  Select,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  useToast,
} from "@/components/ui";
import {
  api,
  type CustomFieldDto,
  type InstalledPluginDto,
  type McpToolDescriptor,
  type SsoProviderDto,
} from "@/lib/api";

// ============================================================================
// 1. Plugins & Sandboxed WASM Marketplace (`/settings/plugins`)
// ============================================================================

export function PluginsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const installedQuery = useQuery({
    queryKey: ["plugins-installed"],
    queryFn: api.listPlugins,
  });

  const marketplaceQuery = useQuery({
    queryKey: ["plugins-marketplace"],
    queryFn: api.listMarketplacePlugins,
  });

  const [invokeTarget, setInvokeTarget] = useState<InstalledPluginDto | null>(null);
  const [invokeText, setInvokeText] = useState(
    "RAPI combines Rust Axum performance with sandboxed WebAssembly plugins and Strapi v5 API parity.",
  );
  const [invokeTitle, setInvokeTitle] = useState("High-Performance Headless CMS Architecture");
  const [invokeMetric, setInvokeMetric] = useState("450");
  const [invokeResult, setInvokeResult] = useState<Record<string, unknown> | null>(null);
  const [configTarget, setConfigTarget] = useState<InstalledPluginDto | null>(null);
  const [configJsonText, setConfigJsonText] = useState("{}");

  const [wasmUploadOpen, setWasmUploadOpen] = useState(false);
  const [wasmName, setWasmName] = useState("");
  const [wasmVersion, setWasmVersion] = useState("0.1.0");
  const [wasmDesc, setWasmDesc] = useState("");
  const [wasmBase64, setWasmBase64] = useState("");

  const toggleMut = useMutation({
    mutationFn: (p: { name: string; enabled: boolean }) =>
      api.updatePlugin(p.name, { enabled: p.enabled }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plugins-installed"] });
      pushToast({ title: "Plugin status updated", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Update failed", description: e.message, tone: "error" }),
  });

  const saveConfigMut = useMutation({
    mutationFn: () => {
      if (!configTarget) throw new Error("No plugin selected");
      const parsed = JSON.parse(configJsonText || "{}") as Record<string, unknown>;
      return api.updatePlugin(configTarget.name, { config: parsed });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plugins-installed"] });
      setConfigTarget(null);
      pushToast({ title: "Plugin configuration saved", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Invalid configuration", description: e.message, tone: "error" }),
  });

  const installMarketMut = useMutation({
    mutationFn: (name: string) => api.installMarketplacePlugin(name),
    onSuccess: (installed) => {
      queryClient.invalidateQueries({ queryKey: ["plugins-installed"] });
      queryClient.invalidateQueries({ queryKey: ["plugins-marketplace"] });
      pushToast({
        title: `Installed ${installed.name}`,
        description: "Verified WASM bytecode & fuel sandbox boundary.",
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "Install failed", description: e.message, tone: "error" }),
  });

  const uploadWasmMut = useMutation({
    mutationFn: () =>
      api.installCustomWasmPlugin({
        name: wasmName,
        version: wasmVersion,
        description: wasmDesc,
        wasmBase64,
      }),
    onSuccess: (installed) => {
      queryClient.invalidateQueries({ queryKey: ["plugins-installed"] });
      setWasmUploadOpen(false);
      setWasmName("");
      setWasmDesc("");
      setWasmBase64("");
      pushToast({
        title: `Installed ${installed.name}`,
        description: "Custom WebAssembly plugin registered.",
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "WASM validation failed", description: e.message, tone: "error" }),
  });

  const uninstallMut = useMutation({
    mutationFn: (name: string) => api.uninstallPlugin(name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plugins-installed"] });
      queryClient.invalidateQueries({ queryKey: ["plugins-marketplace"] });
      pushToast({ title: "Plugin uninstalled", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Uninstall failed", description: e.message, tone: "error" }),
  });

  const invokeMut = useMutation({
    mutationFn: () => {
      if (!invokeTarget) throw new Error("No plugin selected");
      const payload =
        invokeTarget.kind === "wasm"
          ? { input: Number(invokeMetric) || 0, text: invokeText }
          : { title: invokeTitle, text: invokeText };
      return api.invokePlugin(invokeTarget.name, payload);
    },
    onSuccess: (res) => {
      setInvokeResult(res);
    },
    onError: (e: Error) =>
      pushToast({ title: "Execution trapped", description: e.message, tone: "error" }),
  });

  const handleWasmFileSelect = (ev: ChangeEvent<HTMLInputElement>) => {
    const file = ev.target.files?.[0];
    if (!file) return;
    if (!wasmName) {
      setWasmName(file.name.replace(/\.wasm$/i, "").toLowerCase());
    }
    const reader = new FileReader();
    reader.onload = () => {
      const buf = reader.result as ArrayBuffer;
      const bytes = new Uint8Array(buf);
      let bin = "";
      for (const b of bytes) {
        bin += String.fromCharCode(b);
      }
      setWasmBase64(btoa(bin));
    };
    reader.readAsArrayBuffer(file);
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            Plugins &amp; WASM Sandbox
          </h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Compile-time native <code className="font-mono">CmsPlugin</code> extensions and
            runtime-installed <code className="font-mono">wasmi</code> WebAssembly modules (250k
            instruction fuel ceiling, zero ambient WASI host access).
          </p>
        </div>

        <Button size="sm" onClick={() => setWasmUploadOpen(true)}>
          <Upload className="h-3.5 w-3.5" />
          Upload .wasm plugin
        </Button>
      </div>

      <Tabs defaultValue="installed" className="space-y-4">
        <TabsList>
          <TabsTrigger value="installed">
            Installed ({installedQuery.data?.length ?? 0})
          </TabsTrigger>
          <TabsTrigger value="marketplace">
            WASM Marketplace ({marketplaceQuery.data?.length ?? 0})
          </TabsTrigger>
        </TabsList>

        <TabsContent value="installed">
          <Card>
            <DataTable<InstalledPluginDto>
              rows={installedQuery.data ?? []}
              getRowKey={(r) => r.name}
              emptyMessage="No plugins registered."
              columns={[
                {
                  key: "name",
                  header: "Plugin",
                  render: (p) => (
                    <div>
                      <div className="font-mono text-xs font-semibold text-zinc-900">{p.name}</div>
                      <div className="mt-0.5 text-[11px] text-zinc-500">{p.description}</div>
                    </div>
                  ),
                },
                {
                  key: "runtime",
                  header: "Runtime",
                  render: (p) => (
                    <Badge tone={p.kind === "wasm" ? "indigo" : "mono"}>
                      {p.kind === "wasm" ? "WASM Sandbox" : "Native Rust"}
                    </Badge>
                  ),
                },
                {
                  key: "version",
                  header: "Version",
                  render: (p) => <span className="font-mono text-xs">{p.version}</span>,
                },
                {
                  key: "enabled",
                  header: "Enabled",
                  render: (p) => (
                    <Switch
                      checked={p.enabled}
                      onCheckedChange={(v) => toggleMut.mutate({ name: p.name, enabled: v })}
                    />
                  ),
                },
                {
                  key: "actions",
                  header: "",
                  className: "text-right",
                  render: (p) => (
                    <div className="inline-flex items-center gap-1.5">
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                          setConfigTarget(p);
                          setConfigJsonText(JSON.stringify(p.config ?? {}, null, 2));
                        }}
                      >
                        Config
                      </Button>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={!p.enabled}
                        onClick={() => {
                          setInvokeTarget(p);
                          setInvokeResult(null);
                        }}
                      >
                        <Play className="h-3 w-3" />
                        Test
                      </Button>
                      {p.kind === "wasm" && (
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => uninstallMut.mutate(p.name)}
                        >
                          <Trash2 className="h-3.5 w-3.5 text-zinc-400 hover:text-rose-600" />
                        </Button>
                      )}
                    </div>
                  ),
                },
              ]}
            />
          </Card>
        </TabsContent>

        <TabsContent value="marketplace">
          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {(marketplaceQuery.data ?? []).map((item) => (
              <Card key={item.name} className="flex flex-col justify-between">
                <CardHeader className="flex-col items-stretch gap-1.5">
                  <div className="flex items-center justify-between">
                    <Badge tone="indigo">{item.category}</Badge>
                    <Badge tone="mono">v{item.version}</Badge>
                  </div>
                  <CardTitle className="mt-1 font-mono text-sm">{item.name}</CardTitle>
                  <CardDescription>{item.description}</CardDescription>
                </CardHeader>
                <CardContent className="pt-0">
                  {item.installed ? (
                    <Button size="sm" variant="secondary" className="w-full" disabled>
                      <Check className="h-3.5 w-3.5 text-emerald-600" />
                      Installed
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      className="w-full"
                      disabled={installMarketMut.isPending}
                      onClick={() => installMarketMut.mutate(item.name)}
                    >
                      <Download className="h-3.5 w-3.5" />
                      Install WASM module
                    </Button>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        </TabsContent>
      </Tabs>

      {/* Plugin Config JSON Modal */}
      <Dialog open={Boolean(configTarget)} onOpenChange={(o) => !o && setConfigTarget(null)}>
        <DialogContent
          title={configTarget ? `Configure ${configTarget.name}` : "Configure plugin"}
          description="Persists plugin-specific JSON settings in plugin_registry."
        >
          <div className="space-y-3 text-xs">
            <textarea
              rows={6}
              value={configJsonText}
              onChange={(e) => setConfigJsonText(e.target.value)}
              className="w-full rounded-md border border-zinc-300 bg-zinc-950 p-2.5 font-mono text-[11px] text-zinc-100"
            />
            <div className="flex justify-end gap-2">
              <Button variant="secondary" size="sm" onClick={() => setConfigTarget(null)}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={saveConfigMut.isPending}
                onClick={() => saveConfigMut.mutate()}
              >
                Save JSON config
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Invoke Plugin Modal */}
      <Dialog open={Boolean(invokeTarget)} onOpenChange={(o) => !o && setInvokeTarget(null)}>
        <DialogContent
          title={invokeTarget ? `Execute ${invokeTarget.name}` : "Execute plugin"}
          description="Runs inside deterministic fuel-metered sandbox (WASM) or native CmsPlugin trait."
        >
          {invokeTarget && (
            <div className="space-y-3 text-xs">
              {invokeTarget.kind === "wasm" ? (
                <div>
                  <label className="mb-1 block font-medium text-zinc-700">
                    Integer metric input (<code className="font-mono">run(i32) -&gt; i32</code>)
                  </label>
                  <Input
                    type="number"
                    value={invokeMetric}
                    onChange={(e) => setInvokeMetric(e.target.value)}
                  />
                </div>
              ) : (
                <>
                  <div>
                    <label className="mb-1 block font-medium text-zinc-700">Title</label>
                    <Input value={invokeTitle} onChange={(e) => setInvokeTitle(e.target.value)} />
                  </div>
                  <div>
                    <label className="mb-1 block font-medium text-zinc-700">Body text</label>
                    <textarea
                      rows={3}
                      value={invokeText}
                      onChange={(e) => setInvokeText(e.target.value)}
                      className="w-full rounded-md border border-zinc-300 p-2 text-xs"
                    />
                  </div>
                </>
              )}

              <div className="flex justify-end gap-2">
                <Button size="sm" onClick={() => invokeMut.mutate()} disabled={invokeMut.isPending}>
                  <Play className="h-3.5 w-3.5" />
                  {invokeMut.isPending ? "Running..." : "Invoke plugin"}
                </Button>
              </div>

              {invokeResult && (
                <pre className="max-h-56 overflow-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] text-emerald-300">
                  {JSON.stringify(invokeResult, null, 2)}
                </pre>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>

      {/* Custom WASM Upload Modal */}
      <Dialog open={wasmUploadOpen} onOpenChange={setWasmUploadOpen}>
        <DialogContent
          title="Upload custom WebAssembly plugin"
          description="Module must start with \\0asm, be <= 4 MB, and export `run: (i32) -> i32`."
        >
          <form
            className="space-y-3 text-xs"
            onSubmit={(e) => {
              e.preventDefault();
              uploadWasmMut.mutate();
            }}
          >
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Plugin slug</label>
                <Input
                  required
                  placeholder="my-wasm-plugin"
                  value={wasmName}
                  onChange={(e) => setWasmName(e.target.value)}
                />
              </div>
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Version</label>
                <Input
                  required
                  value={wasmVersion}
                  onChange={(e) => setWasmVersion(e.target.value)}
                />
              </div>
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Description</label>
              <Input
                placeholder="Sandboxed scoring or transformation hook"
                value={wasmDesc}
                onChange={(e) => setWasmDesc(e.target.value)}
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Binary (.wasm)</label>
              <Input type="file" accept=".wasm,application/wasm" onChange={handleWasmFileSelect} />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setWasmUploadOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={!wasmBase64 || uploadWasmMut.isPending}>
                Validate &amp; install
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 2. Custom Fields Registry (`/settings/custom-fields`)
// ============================================================================

export function CustomFieldsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const fieldsQuery = useQuery({
    queryKey: ["custom-fields"],
    queryFn: api.listCustomFields,
  });

  const [open, setOpen] = useState(false);
  const [uid, setUid] = useState("custom::rating-stars");
  const [name, setName] = useState("Rating Stars");
  const [underlyingType, setUnderlyingType] = useState("json");
  const [widget, setWidget] = useState("key-value");

  const createMut = useMutation({
    mutationFn: () =>
      api.registerCustomField({
        uid,
        name,
        underlyingType,
        options: { widget },
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["custom-fields"] });
      setOpen(false);
      pushToast({ title: "Custom field registered", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Registration failed", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: (targetUid: string) => api.deleteCustomField(targetUid),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["custom-fields"] });
      pushToast({ title: "Custom field removed", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Cannot delete custom field", description: e.message, tone: "error" }),
  });

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">Custom Fields</h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Extend the Content-Type Builder with typed custom widgets mapped onto SQLite scalar/JSON
            columns.
          </p>
        </div>

        <Button size="sm" onClick={() => setOpen(true)}>
          <Plus className="h-3.5 w-3.5" />
          Register custom field
        </Button>
      </div>

      <Card>
        <DataTable<CustomFieldDto>
          rows={fieldsQuery.data ?? []}
          getRowKey={(r) => r.uid}
          emptyMessage="No custom fields registered."
          columns={[
            {
              key: "uid",
              header: "UID",
              render: (f) => <span className="font-mono text-xs font-semibold">{f.uid}</span>,
            },
            {
              key: "name",
              header: "Display Name",
              render: (f) => <span className="text-xs text-zinc-900">{f.name}</span>,
            },
            {
              key: "underlying",
              header: "Storage Type",
              render: (f) => <Badge tone="indigo">{f.underlyingType}</Badge>,
            },
            {
              key: "options",
              header: "Widget Options",
              render: (f) => (
                <code className="font-mono text-[11px] text-zinc-600">
                  {JSON.stringify(f.options)}
                </code>
              ),
            },
            {
              key: "actions",
              header: "",
              className: "text-right",
              render: (f) =>
                f.uid.startsWith("global::") ? (
                  <Badge tone="neutral">built-in</Badge>
                ) : (
                  <Button size="sm" variant="ghost" onClick={() => deleteMut.mutate(f.uid)}>
                    <Trash2 className="h-3.5 w-3.5 text-zinc-400 hover:text-rose-600" />
                  </Button>
                ),
            },
          ]}
        />
      </Card>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent
          title="Register custom field"
          description="Once registered, select Field Type -> Custom Field in the Content-Type Builder."
        >
          <form
            className="space-y-3 text-xs"
            onSubmit={(e) => {
              e.preventDefault();
              createMut.mutate();
            }}
          >
            <div>
              <label className="mb-1 block font-medium text-zinc-700">
                Custom field UID (<code className="font-mono">namespace::slug</code>)
              </label>
              <Input required value={uid} onChange={(e) => setUid(e.target.value)} />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Display name</label>
              <Input required value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Underlying type</label>
                <Select
                  value={underlyingType}
                  onValueChange={setUnderlyingType}
                  options={[
                    { value: "json", label: "json" },
                    { value: "string", label: "string" },
                    { value: "text", label: "text" },
                    { value: "integer", label: "integer" },
                    { value: "float", label: "float" },
                    { value: "boolean", label: "boolean" },
                  ]}
                />
              </div>
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Editor widget</label>
                <Select
                  value={widget}
                  onValueChange={setWidget}
                  options={[
                    { value: "color", label: "Color swatch picker" },
                    { value: "seo-slug", label: "SEO slug input" },
                    { value: "code", label: "Monospace code editor" },
                    { value: "key-value", label: "Key-value pairs" },
                  ]}
                />
              </div>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={createMut.isPending}>
                Register
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 3. OIDC Single Sign-On (`/settings/sso`)
// ============================================================================

export function SsoSettingsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const providersQuery = useQuery({
    queryKey: ["sso-config"],
    queryFn: api.listSsoConfig,
  });

  const rolesQuery = useQuery({
    queryKey: ["admin-roles"],
    queryFn: api.listRoles,
  });

  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [issuerUrl, setIssuerUrl] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [scopes, setScopes] = useState("openid email profile");
  const [defaultRoleId, setDefaultRoleId] = useState("role_editor");
  const [autoProvision, setAutoProvision] = useState(true);

  const createMut = useMutation({
    mutationFn: () =>
      api.createSsoProvider({
        name,
        issuerUrl,
        clientId,
        clientSecret,
        scopes,
        defaultRoleId,
        autoProvision,
        isEnabled: true,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["sso-config"] });
      queryClient.invalidateQueries({ queryKey: ["sso-public"] });
      setOpen(false);
      setName("");
      setIssuerUrl("");
      setClientId("");
      setClientSecret("");
      pushToast({ title: "OIDC provider added", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Failed to save OIDC provider", description: e.message, tone: "error" }),
  });

  const toggleSsoMut = useMutation({
    mutationFn: (p: { id: string; isEnabled: boolean }) =>
      api.updateSsoProvider(p.id, { isEnabled: p.isEnabled }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["sso-config"] });
      queryClient.invalidateQueries({ queryKey: ["sso-public"] });
      pushToast({ title: "OIDC provider updated", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Update failed", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => api.deleteSsoProvider(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["sso-config"] });
      queryClient.invalidateQueries({ queryKey: ["sso-public"] });
      pushToast({ title: "OIDC provider deleted", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Delete failed", description: e.message, tone: "error" }),
  });

  const callbackExample =
    typeof window !== "undefined" ? `${window.location.origin}/login` : "http://localhost:5173/login";

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            Single Sign-On (OIDC)
          </h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Authenticate workspace administrators via OpenID Connect (Okta, Google Workspace, Entra
            ID, Keycloak) with CSRF state validation and automatic role provisioning.
          </p>
        </div>

        <Button size="sm" onClick={() => setOpen(true)}>
          <Plus className="h-3.5 w-3.5" />
          Add OIDC provider
        </Button>
      </div>

      <Card>
        <CardHeader className="flex-col items-start gap-1">
          <CardTitle className="text-xs">Redirect URI Configuration</CardTitle>
          <CardDescription>
            Register <code className="font-mono text-zinc-900">{callbackExample}</code> in your
            identity provider&apos;s allowed callback URLs.
          </CardDescription>
        </CardHeader>
        <DataTable<SsoProviderDto>
          rows={providersQuery.data ?? []}
          getRowKey={(r) => r.id}
          emptyMessage="No OIDC providers configured. Password + Argon2id sign-in remains active."
          columns={[
            {
              key: "name",
              header: "Provider",
              render: (p) => (
                <div>
                  <div className="font-medium text-zinc-900">{p.name}</div>
                  <div className="font-mono text-[11px] text-zinc-500">{p.issuerUrl}</div>
                </div>
              ),
            },
            {
              key: "clientId",
              header: "Client ID",
              render: (p) => <span className="font-mono text-xs">{p.clientId}</span>,
            },
            {
              key: "role",
              header: "Default Role",
              render: (p) => <Badge tone="mono">{p.defaultRoleId}</Badge>,
            },
            {
              key: "provision",
              header: "Auto-Provision",
              render: (p) => (
                <Badge tone={p.autoProvision ? "emerald" : "neutral"}>
                  {p.autoProvision ? "enabled" : "existing only"}
                </Badge>
              ),
            },
            {
              key: "enabled",
              header: "Enabled",
              render: (p) => (
                <Switch
                  checked={p.isEnabled}
                  onCheckedChange={(v) => toggleSsoMut.mutate({ id: p.id, isEnabled: v })}
                />
              ),
            },
            {
              key: "actions",
              header: "",
              className: "text-right",
              render: (p) => (
                <Button size="sm" variant="ghost" onClick={() => deleteMut.mutate(p.id)}>
                  <Trash2 className="h-3.5 w-3.5 text-zinc-400 hover:text-rose-600" />
                </Button>
              ),
            },
          ]}
        />
      </Card>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent
          title="Configure OIDC Identity Provider"
          description="Discovers endpoints automatically via `.well-known/openid-configuration`."
        >
          <form
            className="space-y-3 text-xs"
            onSubmit={(e) => {
              e.preventDefault();
              createMut.mutate();
            }}
          >
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Provider name</label>
              <Input
                required
                placeholder="Okta Corporate SSO"
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </div>
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Issuer URL</label>
              <Input
                required
                placeholder="https://sso.example.com"
                value={issuerUrl}
                onChange={(e) => setIssuerUrl(e.target.value)}
              />
            </div>
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Client ID</label>
                <Input
                  required
                  value={clientId}
                  onChange={(e) => setClientId(e.target.value)}
                />
              </div>
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Client Secret</label>
                <Input
                  required
                  type="password"
                  value={clientSecret}
                  onChange={(e) => setClientSecret(e.target.value)}
                />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Scopes</label>
                <Input value={scopes} onChange={(e) => setScopes(e.target.value)} />
              </div>
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Default Admin Role</label>
                <Select
                  value={defaultRoleId}
                  onValueChange={setDefaultRoleId}
                  options={(rolesQuery.data ?? []).map((r) => ({
                    value: r.id,
                    label: r.name,
                  }))}
                />
              </div>
            </div>
            <label className="flex items-center justify-between rounded border border-zinc-200 p-2.5">
              <span>Auto-provision first-time verified OIDC users</span>
              <Switch checked={autoProvision} onCheckedChange={setAutoProvision} />
            </label>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={createMut.isPending}>
                Save provider
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ============================================================================
// 4. Opt-in AI Editorial Assistant (`/settings/ai`)
// ============================================================================

export function AiSettingsPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const aiQuery = useQuery({
    queryKey: ["ai-settings"],
    queryFn: api.getAiSettings,
  });

  const [isEnabled, setIsEnabled] = useState(false);
  const [provider, setProvider] = useState("local-deterministic");
  const [apiUrl, setApiUrl] = useState("https://api.openai.com/v1");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("gpt-4o-mini");

  const [task, setTask] = useState<"translate" | "generate_draft" | "seo_summary">("seo_summary");
  const [targetLocale, setTargetLocale] = useState("fr");
  const [sampleText, setSampleText] = useState(
    "RAPI delivers full Strapi v5 REST and GraphQL parity in a single self-contained Rust binary backed by SQLite WAL.",
  );
  const [aiResult, setAiResult] = useState<Record<string, unknown> | null>(null);

  useEffect(() => {
    if (aiQuery.data) {
      setIsEnabled(aiQuery.data.isEnabled);
      setProvider(aiQuery.data.provider);
      setApiUrl(aiQuery.data.apiUrl);
      setModel(aiQuery.data.model);
    }
  }, [aiQuery.data]);

  const saveMut = useMutation({
    mutationFn: () =>
      api.updateAiSettings({
        isEnabled,
        provider,
        apiUrl,
        model,
        ...(apiKey.trim() ? { apiKey: apiKey.trim() } : {}),
      }),
    onSuccess: () => {
      setApiKey("");
      queryClient.invalidateQueries({ queryKey: ["ai-settings"] });
      pushToast({ title: "AI configuration saved", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Save failed", description: e.message, tone: "error" }),
  });

  const runMut = useMutation({
    mutationFn: () =>
      api.completeAiTask({
        task,
        content: sampleText,
        targetLocale: task === "translate" ? targetLocale : undefined,
      }),
    onSuccess: (out) => setAiResult(out.result),
    onError: (e: Error) =>
      pushToast({ title: "AI task rejected", description: e.message, tone: "error" }),
  });

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
          Opt-in AI Editorial Assistant
        </h1>
        <p className="mt-0.5 text-xs text-zinc-600">
          Disabled by default for strict data privacy. Enable locally (zero-egress deterministic
          engine) or connect any OpenAI-compatible <code className="font-mono">/chat/completions</code>{" "}
          endpoint.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
        <Card className="lg:col-span-6">
          <CardHeader>
            <CardTitle>Provider Configuration</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3.5 text-xs">
            <label className="flex items-center justify-between rounded border border-zinc-200 bg-zinc-50 p-3">
              <div>
                <div className="font-semibold text-zinc-900">Enable AI Editorial Assistant</div>
                <div className="text-[11px] text-zinc-500">
                  Exposes AI Assist toolbar inside the Content Manager entry editor.
                </div>
              </div>
              <Switch checked={isEnabled} onCheckedChange={setIsEnabled} />
            </label>

            <div>
              <label className="mb-1 block font-medium text-zinc-700">Provider</label>
              <Select
                value={provider}
                onValueChange={setProvider}
                options={[
                  {
                    value: "local-deterministic",
                    label: "Local Deterministic (Offline / Zero external calls)",
                  },
                  {
                    value: "openai-compatible",
                    label: "OpenAI-Compatible (/v1/chat/completions)",
                  },
                ]}
              />
            </div>

            {provider === "openai-compatible" && (
              <>
                <div>
                  <label className="mb-1 block font-medium text-zinc-700">Base API URL</label>
                  <Input value={apiUrl} onChange={(e) => setApiUrl(e.target.value)} />
                </div>
                <div>
                  <label className="mb-1 block font-medium text-zinc-700">
                    API Key {aiQuery.data?.hasApiKey && "(configured — leave blank to keep)"}
                  </label>
                  <Input
                    type="password"
                    placeholder="sk-..."
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                  />
                </div>
              </>
            )}

            <div>
              <label className="mb-1 block font-medium text-zinc-700">Model identifier</label>
              <Input value={model} onChange={(e) => setModel(e.target.value)} />
            </div>

            <Button size="sm" onClick={() => saveMut.mutate()} disabled={saveMut.isPending}>
              Save AI settings
            </Button>
          </CardContent>
        </Card>

        <Card className="lg:col-span-6">
          <CardHeader className="flex-col items-start gap-1">
            <CardTitle>Interactive Editorial Playground</CardTitle>
            <CardDescription>Test translation, draft generation, and SEO summaries.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="mb-1 block font-medium text-zinc-700">Task</label>
                <Select
                  value={task}
                  onValueChange={(v) =>
                    setTask(v as "translate" | "generate_draft" | "seo_summary")
                  }
                  options={[
                    { value: "seo_summary", label: "SEO Metadata & Keywords" },
                    { value: "translate", label: "Locale Translation" },
                    { value: "generate_draft", label: "Structured Draft Outline" },
                  ]}
                />
              </div>
              {task === "translate" && (
                <div>
                  <label className="mb-1 block font-medium text-zinc-700">Target locale</label>
                  <Input value={targetLocale} onChange={(e) => setTargetLocale(e.target.value)} />
                </div>
              )}
            </div>

            <div>
              <label className="mb-1 block font-medium text-zinc-700">Source copy</label>
              <textarea
                rows={3}
                value={sampleText}
                onChange={(e) => setSampleText(e.target.value)}
                className="w-full rounded-md border border-zinc-300 p-2 text-xs"
              />
            </div>

            <Button
              size="sm"
              variant="secondary"
              disabled={runMut.isPending}
              onClick={() => runMut.mutate()}
            >
              <Sparkles className="h-3.5 w-3.5 text-indigo-700" />
              {runMut.isPending ? "Generating..." : "Run AI task"}
            </Button>

            {aiResult && (
              <pre className="max-h-52 overflow-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] text-zinc-100">
                {JSON.stringify(aiResult, null, 2)}
              </pre>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

// ============================================================================
// 5. General Platform Settings & MCP Server Inspector (`/settings/general`)
// ============================================================================

export function GeneralSettingsPage() {
  const { pushToast } = useToast();
  const [copied, setCopied] = useState(false);
  const [selectedTool, setSelectedTool] = useState("list_content_types");
  const [toolArgsRaw, setToolArgsRaw] = useState("{}");
  const [mcpOutput, setMcpOutput] = useState<Record<string, unknown> | null>(null);

  const overviewQuery = useQuery({
    queryKey: ["system-overview"],
    queryFn: api.getSystemOverview,
  });

  const readinessQuery = useQuery({
    queryKey: ["readiness"],
    queryFn: api.getReadiness,
  });

  const toolsQuery = useQuery({
    queryKey: ["mcp-tools"],
    queryFn: async () => {
      const res = await api.callMcpRpc({
        jsonrpc: "2.0",
        id: 1,
        method: "tools/list",
        params: {},
      });
      const result = res.result as { tools?: McpToolDescriptor[] } | undefined;
      return result?.tools ?? [];
    },
  });

  const callToolMut = useMutation({
    mutationFn: async () => {
      const parsedArgs = JSON.parse(toolArgsRaw || "{}") as Record<string, unknown>;
      return api.callMcpRpc({
        jsonrpc: "2.0",
        id: Date.now(),
        method: "tools/call",
        params: {
          name: selectedTool,
          arguments: parsedArgs,
        },
      });
    },
    onSuccess: (res) => setMcpOutput(res),
    onError: (e: Error) =>
      pushToast({ title: "MCP call failed", description: e.message, tone: "error" }),
  });

  const mcpStdioSnippet = JSON.stringify(
    {
      mcpServers: {
        "rapi-cms": {
          command: "rapi",
          args: ["mcp"],
          env: {
            DATABASE_URL: "sqlite://./data/rapi.db?mode=rwc",
          },
        },
      },
    },
    null,
    2,
  );

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
          Platform Settings &amp; MCP Server
        </h1>
        <p className="mt-0.5 text-xs text-zinc-600">
          Runtime topology, extensibility modules, and built-in Model Context Protocol (JSON-RPC 2.0
          over HTTP <code className="font-mono">/api/mcp</code> and{" "}
          <code className="font-mono">rapi mcp</code> stdio).
        </p>
      </div>

      {/* Extensibility Quick Cards */}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Link
          to="/settings/plugins"
          className="rounded-md border border-zinc-200 bg-white p-3.5 transition-colors hover:border-indigo-400"
        >
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-900">
            <Plug className="h-4 w-4 text-indigo-700" />
            Plugins &amp; WASM
          </div>
          <p className="mt-1 text-[11px] text-zinc-500">
            Native Rust plugins &amp; fuel-metered WebAssembly marketplace.
          </p>
        </Link>
        <Link
          to="/settings/custom-fields"
          className="rounded-md border border-zinc-200 bg-white p-3.5 transition-colors hover:border-indigo-400"
        >
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-900">
            <Layers className="h-4 w-4 text-indigo-700" />
            Custom Fields
          </div>
          <p className="mt-1 text-[11px] text-zinc-500">
            Color pickers, code editors, SEO slugs, and custom JSON widgets.
          </p>
        </Link>
        <Link
          to="/settings/sso"
          className="rounded-md border border-zinc-200 bg-white p-3.5 transition-colors hover:border-indigo-400"
        >
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-900">
            <ShieldCheck className="h-4 w-4 text-indigo-700" />
            OIDC Single Sign-On
          </div>
          <p className="mt-1 text-[11px] text-zinc-500">
            Enterprise identity providers &amp; automatic role mapping.
          </p>
        </Link>
        <Link
          to="/settings/ai"
          className="rounded-md border border-zinc-200 bg-white p-3.5 transition-colors hover:border-indigo-400"
        >
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-900">
            <Bot className="h-4 w-4 text-indigo-700" />
            Opt-in AI Assistant
          </div>
          <p className="mt-1 text-[11px] text-zinc-500">
            Privacy-first editorial translation, drafting, and SEO metadata.
          </p>
        </Link>
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
        {/* Runtime Summary */}
        <Card className="lg:col-span-5">
          <CardHeader>
            <div className="flex items-center gap-2">
              <Cpu className="h-4 w-4 text-indigo-700" />
              <CardTitle>Active Runtime &amp; Storage</CardTitle>
            </div>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <div className="divide-y divide-zinc-200 rounded border border-zinc-200">
              <div className="flex items-center justify-between px-3 py-2">
                <span className="text-zinc-500">Version</span>
                <Badge tone="mono">v{overviewQuery.data?.version ?? "0.1.0"}</Badge>
              </div>
              <div className="flex items-center justify-between px-3 py-2">
                <span className="text-zinc-500">SQLite WAL Pool</span>
                <Badge tone="emerald">{readinessQuery.data?.database ?? "connected"}</Badge>
              </div>
              <div className="flex items-center justify-between px-3 py-2">
                <span className="text-zinc-500">Cache Adapter</span>
                <Badge tone="mono">{readinessQuery.data?.cacheBackend ?? "memory"}</Badge>
              </div>
              <div className="flex items-center justify-between px-3 py-2">
                <span className="text-zinc-500">Queue Adapter</span>
                <Badge tone="mono">{readinessQuery.data?.queueBackend ?? "sqlite"}</Badge>
              </div>
            </div>

            <div>
              <div className="mb-1 flex items-center justify-between">
                <span className="font-medium text-zinc-700">
                  MCP Client Configuration (<code className="font-mono">rapi mcp</code>)
                </span>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(mcpStdioSnippet);
                    setCopied(true);
                    setTimeout(() => setCopied(false), 1500);
                  }}
                  className="inline-flex items-center gap-1 text-[11px] text-indigo-700 hover:underline cursor-pointer"
                >
                  {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
                  {copied ? "Copied" : "Copy JSON"}
                </button>
              </div>
              <pre className="overflow-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] text-zinc-100">
                {mcpStdioSnippet}
              </pre>
            </div>
          </CardContent>
        </Card>

        {/* Live MCP Server Inspector */}
        <Card className="lg:col-span-7">
          <CardHeader className="flex-col items-stretch gap-1">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Terminal className="h-4 w-4 text-indigo-700" />
                <CardTitle>Live MCP Server Inspector (/api/mcp)</CardTitle>
              </div>
              <Badge tone="indigo">{toolsQuery.data?.length ?? 8} RBAC-scoped tools</Badge>
            </div>
            <CardDescription>
              Invoke JSON-RPC 2.0 MCP tools directly with your active administrator permissions.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <div>
              <label className="mb-1 block font-medium text-zinc-700">Tool</label>
              <Select
                value={selectedTool}
                onValueChange={(v) => {
                  setSelectedTool(v);
                  if (v === "list_content_types" || v === "list_media_files") {
                    setToolArgsRaw("{}");
                  } else if (v === "get_content_type_schema" || v === "list_documents") {
                    setToolArgsRaw('{\n  "uid": "api::article.article"\n}');
                  }
                }}
                options={(toolsQuery.data ?? []).map((t) => ({
                  value: t.name,
                  label: `${t.name} — ${t.description}`,
                }))}
              />
            </div>

            <div>
              <label className="mb-1 block font-medium text-zinc-700">Arguments (JSON)</label>
              <textarea
                rows={3}
                value={toolArgsRaw}
                onChange={(e) => setToolArgsRaw(e.target.value)}
                className="w-full rounded-md border border-zinc-300 bg-zinc-950 p-2.5 font-mono text-[11px] text-zinc-100"
              />
            </div>

            <div className="flex items-center justify-between">
              <span className="inline-flex items-center gap-1 text-[11px] text-zinc-500">
                <KeyRound className="h-3 w-3" />
                Also accessible to external agents via <code className="font-mono">Bearer rapi_api_...</code>
              </span>
              <Button
                size="sm"
                onClick={() => callToolMut.mutate()}
                disabled={callToolMut.isPending}
              >
                <Play className="h-3.5 w-3.5" />
                {callToolMut.isPending ? "Calling..." : "Execute MCP tool"}
              </Button>
            </div>

            {mcpOutput && (
              <pre className="max-h-60 overflow-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] text-emerald-300">
                {JSON.stringify(mcpOutput, null, 2)}
              </pre>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
