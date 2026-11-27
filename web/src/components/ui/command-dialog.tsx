import * as Dialog from "@radix-ui/react-dialog";
import {
  Blocks,
  Database,
  FileJson,
  FolderOpen,
  Globe,
  KeyRound,
  LayoutDashboard,
  Search,
  ShieldCheck,
  Webhook,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useUiStore } from "@/app/store";

const COMMAND_ITEMS = [
  { label: "System overview", path: "/", group: "Workspace", icon: LayoutDashboard },
  { label: "Content Manager", path: "/content-manager", group: "Workspace", icon: Database },
  { label: "Content-Type Builder", path: "/content-type-builder", group: "Builder", icon: Blocks },
  { label: "Media Library", path: "/media-library", group: "Workspace", icon: FolderOpen },
  { label: "Roles & Permissions", path: "/users", group: "Governance", icon: ShieldCheck },
  { label: "API Tokens", path: "/settings/api-tokens", group: "Settings", icon: KeyRound },
  { label: "Webhooks", path: "/settings/webhooks", group: "Settings", icon: Webhook },
  { label: "Internationalization (i18n)", path: "/settings/i18n", group: "Settings", icon: Globe },
  { label: "First-admin onboarding", path: "/onboarding", group: "Auth", icon: ShieldCheck },
  { label: "OpenAPI 3.1 JSON specification", path: "/openapi.json", external: true, group: "API", icon: FileJson },
];

export function CommandDialog() {
  const { commandOpen, setCommandOpen } = useUiStore();
  const [query, setQuery] = useState("");
  const navigate = useNavigate();

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCommandOpen(!commandOpen);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [commandOpen, setCommandOpen]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return COMMAND_ITEMS;
    return COMMAND_ITEMS.filter(
      (item) => item.label.toLowerCase().includes(q) || item.group.toLowerCase().includes(q),
    );
  }, [query]);

  return (
    <Dialog.Root open={commandOpen} onOpenChange={setCommandOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-zinc-950/35 backdrop-blur-[1px]" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-[16%] z-50 w-full max-w-lg -translate-x-1/2 rounded-lg border border-zinc-200 bg-white shadow-xl focus:outline-none"
        >
          <Dialog.Title className="sr-only">Jump to view</Dialog.Title>
          <div className="flex items-center gap-2 border-b border-zinc-200 px-3 py-2.5">
            <Search className="h-4 w-4 text-zinc-400" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Jump to module, setting, or API resource..."
              className="w-full bg-transparent text-xs text-zinc-900 placeholder:text-zinc-400 focus:outline-none"
            />
            <kbd className="rounded border border-zinc-200 bg-zinc-100 px-1.5 py-0.5 font-mono text-[10px] text-zinc-500">
              ESC
            </kbd>
          </div>
          <div className="max-h-72 overflow-y-auto p-1.5">
            {filtered.length === 0 ? (
              <p className="px-3 py-6 text-center text-xs text-zinc-500">
                No matching destinations for "{query}".
              </p>
            ) : (
              filtered.map((item) => {
                const Icon = item.icon;
                return (
                  <button
                    key={item.path}
                    type="button"
                    onClick={() => {
                      setCommandOpen(false);
                      setQuery("");
                      if (item.external) {
                        window.open(item.path, "_blank", "noopener,noreferrer");
                      } else {
                        navigate(item.path);
                      }
                    }}
                    className="flex w-full items-center justify-between rounded-md px-2.5 py-2 text-left text-xs text-zinc-700 hover:bg-indigo-50 hover:text-indigo-900 cursor-pointer"
                  >
                    <span className="flex items-center gap-2.5">
                      <Icon className="h-3.5 w-3.5 text-zinc-500" />
                      <span className="font-medium">{item.label}</span>
                    </span>
                    <span className="font-mono text-[11px] text-zinc-400">{item.path}</span>
                  </button>
                );
              })
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
