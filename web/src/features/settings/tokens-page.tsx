import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Copy, KeyRound, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  Checkbox,
  DataTable,
  Dialog,
  DialogContent,
  DialogTrigger,
  Input,
  Select,
  Tabs,
  TabsList,
  TabsTrigger,
  useToast,
} from "@/components/ui";
import { type ApiTokenDto, api, type TokenType } from "@/lib/api";

const LIFESPAN_OPTIONS = [
  { value: "unlimited", label: "Unlimited" },
  { value: String(7 * 86400 * 1000), label: "7 days" },
  { value: String(30 * 86400 * 1000), label: "30 days" },
  { value: String(90 * 86400 * 1000), label: "90 days" },
];

export function TokensPage({ defaultKind = "api" }: { defaultKind?: "api" | "admin" }) {
  const [kind, setKind] = useState<"api" | "admin">(defaultKind);
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  useEffect(() => {
    setKind(defaultKind);
  }, [defaultKind]);

  const tokensQuery = useQuery({
    queryKey: ["tokens", kind],
    queryFn: () => api.listTokens(kind),
  });

  const catalogQuery = useQuery({
    queryKey: ["permission-catalog"],
    queryFn: api.getPermissionCatalog,
  });

  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [tokenType, setTokenType] = useState<TokenType>("read-only");
  const [lifespan, setLifespan] = useState("unlimited");
  const [customPerms, setCustomPerms] = useState<Set<string>>(new Set());
  const [revealedKey, setRevealedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const createMut = useMutation({
    mutationFn: () =>
      api.createToken(kind, {
        name,
        description,
        type: tokenType,
        lifespanMs: lifespan === "unlimited" ? null : Number(lifespan),
        permissions:
          tokenType === "custom"
            ? Array.from(customPerms).map((action) => ({ action }))
            : [],
      }),
    onSuccess: (created) => {
      queryClient.invalidateQueries({ queryKey: ["tokens", kind] });
      setRevealedKey(created.accessKey);
      setOpen(false);
      setName("");
      setDescription("");
      setCustomPerms(new Set());
      pushToast({ title: "Token created — copy key now", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Failed to create token", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => api.deleteToken(kind, id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["tokens", kind] });
      pushToast({ title: "Token revoked", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Revoke failed", description: e.message, tone: "error" }),
  });

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            API &amp; Admin Tokens
          </h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Bearer tokens are SHA-256 hashed at rest and cached in memory/Redis for sub-millisecond
            verification.
          </p>
        </div>

        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button size="sm">
              <Plus className="h-3.5 w-3.5" />
              Create {kind === "admin" ? "admin" : "API"} token
            </Button>
          </DialogTrigger>
          <DialogContent
            title={`Create ${kind === "admin" ? "Admin" : "API"} token`}
            description="The raw secret key is shown only once immediately after creation."
          >
            <form
              className="space-y-3"
              onSubmit={(e) => {
                e.preventDefault();
                createMut.mutate();
              }}
            >
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">Name</label>
                <Input
                  required
                  placeholder="Next.js Production Read"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">Description</label>
                <Input
                  placeholder="Used by storefront ISR builds"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">Scope</label>
                  <Select
                    value={tokenType}
                    onValueChange={(v) => setTokenType(v as TokenType)}
                    options={[
                      { value: "read-only", label: "Read-only" },
                      { value: "full-access", label: "Full access" },
                      { value: "custom", label: "Custom" },
                    ]}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">Duration</label>
                  <Select
                    value={lifespan}
                    onValueChange={setLifespan}
                    options={LIFESPAN_OPTIONS}
                  />
                </div>
              </div>
              {tokenType === "custom" && (
                <div className="space-y-1.5">
                  <label className="block text-xs font-medium text-zinc-700">
                    Allowed permissions ({customPerms.size} selected)
                  </label>
                  <div className="max-h-40 space-y-1 overflow-y-auto rounded border border-zinc-200 p-2">
                    {(catalogQuery.data ?? []).map((item) => (
                      <label
                        key={item.action}
                        className="flex cursor-pointer items-center gap-2 rounded px-1.5 py-1 text-xs hover:bg-zinc-50"
                      >
                        <Checkbox
                          checked={customPerms.has(item.action)}
                          onCheckedChange={(checked) => {
                            const next = new Set(customPerms);
                            if (checked) next.add(item.action);
                            else next.delete(item.action);
                            setCustomPerms(next);
                          }}
                        />
                        <span className="text-zinc-800">{item.label}</span>
                      </label>
                    ))}
                  </div>
                </div>
              )}
              <div className="flex justify-end gap-2 pt-2">
                <Button type="button" variant="secondary" onClick={() => setOpen(false)}>
                  Cancel
                </Button>
                <Button type="submit" disabled={createMut.isPending}>
                  Generate token
                </Button>
              </div>
            </form>
          </DialogContent>
        </Dialog>
      </div>

      {revealedKey && (
        <div className="rounded-md border border-amber-300 bg-amber-50 p-4 text-xs text-amber-950">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 font-semibold">
              <KeyRound className="h-4 w-4 text-amber-700" />
              Copy this token now — it will never be displayed again.
            </div>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                navigator.clipboard.writeText(revealedKey);
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
              }}
            >
              {copied ? <Check className="h-3.5 w-3.5 text-emerald-600" /> : <Copy className="h-3.5 w-3.5" />}
              {copied ? "Copied" : "Copy key"}
            </Button>
          </div>
          <code className="mt-2 block overflow-x-auto rounded border border-amber-200 bg-white px-3 py-2 font-mono text-[11px] text-zinc-900">
            {revealedKey}
          </code>
        </div>
      )}

      <Tabs value={kind} onValueChange={(v) => setKind(v as "api" | "admin")}>
        <TabsList>
          <TabsTrigger value="api">Content API Tokens</TabsTrigger>
          <TabsTrigger value="admin">Admin Tokens</TabsTrigger>
        </TabsList>
      </Tabs>

      <DataTable<ApiTokenDto>
        rows={tokensQuery.data ?? []}
        getRowKey={(t) => t.id}
        emptyMessage={`No ${kind === "admin" ? "admin" : "API"} tokens created yet.`}
        columns={[
          {
            key: "name",
            header: "Name",
            render: (t) => (
              <div>
                <div className="font-medium text-zinc-900">{t.name}</div>
                {t.description && (
                  <div className="text-[11px] text-zinc-500">{t.description}</div>
                )}
              </div>
            ),
          },
          {
            key: "type",
            header: "Scope",
            render: (t) => (
              <Badge
                tone={
                  t.tokenType === "full-access"
                    ? "amber"
                    : t.tokenType === "read-only"
                      ? "emerald"
                      : "indigo"
                }
              >
                {t.tokenType}
              </Badge>
            ),
          },
          {
            key: "expires",
            header: "Expires",
            render: (t) => (
              <span className="font-mono text-[11px] text-zinc-600">
                {t.expiresAt ? new Date(t.expiresAt).toLocaleDateString() : "Never"}
              </span>
            ),
          },
          {
            key: "created",
            header: "Created",
            render: (t) => (
              <span className="font-mono text-[11px] text-zinc-500">
                {new Date(t.createdAt).toLocaleDateString()}
              </span>
            ),
          },
          {
            key: "actions",
            header: "",
            className: "text-right",
            render: (t) => (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => deleteMut.mutate(t.id)}
                disabled={deleteMut.isPending}
              >
                <Trash2 className="h-3.5 w-3.5 text-zinc-500 hover:text-rose-600" />
              </Button>
            ),
          },
        ]}
      />
    </div>
  );
}
