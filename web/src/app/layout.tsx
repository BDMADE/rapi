import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { useQuery } from "@tanstack/react-query";
import {
  Blocks,
  ChevronDown,
  Database,
  FolderOpen,
  Globe,
  KeyRound,
  LayoutDashboard,
  LogIn,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Settings,
  ShieldCheck,
  Webhook,
} from "lucide-react";
import { Link, NavLink, Outlet, useLocation } from "react-router-dom";
import { useUiStore } from "@/app/store";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CommandDialog } from "@/components/ui/command-dialog";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";

const NAV_GROUPS = [
  {
    title: "Content",
    items: [
      { label: "Overview", to: "/", icon: LayoutDashboard, end: true },
      { label: "Content Manager", to: "/content-manager", icon: Database },
      { label: "Media Library", to: "/media-library", icon: FolderOpen },
    ],
  },
  {
    title: "Architecture",
    items: [
      { label: "Content-Type Builder", to: "/content-type-builder", icon: Blocks },
      { label: "Roles & Access", to: "/users", icon: ShieldCheck },
    ],
  },
  {
    title: "Configuration",
    items: [
      { label: "API Tokens", to: "/settings/api-tokens", icon: KeyRound },
      { label: "Webhooks", to: "/settings/webhooks", icon: Webhook },
      { label: "Locales (i18n)", to: "/settings/i18n", icon: Globe },
      { label: "Platform Settings", to: "/settings/general", icon: Settings },
    ],
  },
];

export function AppLayout() {
  const { sidebarCollapsed, toggleSidebar, activeLocale, setActiveLocale, setCommandOpen } =
    useUiStore();
  const location = useLocation();

  const readinessQuery = useQuery({
    queryKey: ["readiness"],
    queryFn: api.getReadiness,
    refetchInterval: 15_000,
    retry: 1,
  });

  const initQuery = useQuery({
    queryKey: ["admin-init"],
    queryFn: api.getAdminInit,
    retry: 1,
  });

  const isOnline = readinessQuery.isSuccess && readinessQuery.data.status === "ready";

  return (
    <div className="flex min-h-screen bg-zinc-50 text-zinc-900">
      <CommandDialog />

      {/* Left structural navigation rail */}
      <aside
        className={cn(
          "sticky top-0 flex h-screen shrink-0 flex-col border-r border-zinc-200 bg-zinc-100/80 transition-[width] duration-150",
          sidebarCollapsed ? "w-14" : "w-60",
        )}
      >
        <div className="flex h-12 items-center justify-between border-b border-zinc-200 px-3">
          <Link to="/" className="flex items-center gap-2 overflow-hidden">
            <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-indigo-700 font-mono text-xs font-semibold text-white">
              R
            </div>
            {!sidebarCollapsed && (
              <div className="flex items-baseline gap-1.5 truncate">
                <span className="text-sm font-semibold tracking-tight text-zinc-900">RAPI</span>
                <span className="font-mono text-[10px] text-zinc-500">
                  v{initQuery.data?.version ?? "0.1.0"}
                </span>
              </div>
            )}
          </Link>
          <button
            type="button"
            onClick={toggleSidebar}
            title={sidebarCollapsed ? "Expand navigation" : "Collapse navigation"}
            className="rounded p-1 text-zinc-500 hover:bg-zinc-200/70 hover:text-zinc-900 cursor-pointer"
          >
            {sidebarCollapsed ? (
              <PanelLeftOpen className="h-4 w-4" />
            ) : (
              <PanelLeftClose className="h-4 w-4" />
            )}
          </button>
        </div>

        <nav className="flex-1 space-y-5 overflow-y-auto px-2 py-3">
          {NAV_GROUPS.map((group) => (
            <div key={group.title} className="space-y-0.5">
              {!sidebarCollapsed && (
                <div className="px-2 pb-1 text-[11px] font-medium text-zinc-500">
                  {group.title}
                </div>
              )}
              {group.items.map((item) => {
                const Icon = item.icon;
                return (
                  <NavLink
                    key={item.to}
                    to={item.to}
                    end={item.end}
                    title={sidebarCollapsed ? item.label : undefined}
                    className={({ isActive }) =>
                      cn(
                        "flex items-center gap-2.5 rounded-md px-2 py-1.5 text-xs font-medium transition-colors",
                        isActive
                          ? "bg-white text-indigo-700 shadow-[0_1px_1px_rgba(0,0,0,0.04)] ring-1 ring-zinc-200/90"
                          : "text-zinc-600 hover:bg-zinc-200/60 hover:text-zinc-900",
                        sidebarCollapsed && "justify-center px-0",
                      )
                    }
                  >
                    <Icon className="h-4 w-4 shrink-0" />
                    {!sidebarCollapsed && <span className="truncate">{item.label}</span>}
                  </NavLink>
                );
              })}
            </div>
          ))}
        </nav>

        <div className="border-t border-zinc-200 p-2">
          <Link
            to="/login"
            className={cn(
              "flex items-center gap-2 rounded-md px-2 py-1.5 text-xs text-zinc-600 hover:bg-zinc-200/60 hover:text-zinc-900",
              sidebarCollapsed && "justify-center px-0",
            )}
          >
            <LogIn className="h-4 w-4 shrink-0 text-zinc-500" />
            {!sidebarCollapsed && <span className="truncate">Sign in / Switch user</span>}
          </Link>
        </div>
      </aside>

      {/* Main workspace column */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Slim 48px command topbar */}
        <header className="sticky top-0 z-20 flex h-12 items-center justify-between border-b border-zinc-200 bg-white/95 px-5 backdrop-blur-xs">
          <div className="flex items-center gap-2 text-xs text-zinc-500">
            <span className="font-medium text-zinc-800">Workspace</span>
            <span>/</span>
            <span className="font-mono text-[11px] text-zinc-600">{location.pathname}</span>
          </div>

          <div className="flex items-center gap-2.5">
            <button
              type="button"
              onClick={() => setCommandOpen(true)}
              className="flex h-7 items-center gap-2 rounded-md border border-zinc-200 bg-zinc-50 px-2.5 text-xs text-zinc-500 hover:border-zinc-300 hover:text-zinc-800 cursor-pointer"
            >
              <Search className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">Jump to...</span>
              <kbd className="rounded border border-zinc-200 bg-white px-1 font-mono text-[10px] text-zinc-500">
                ⌘K
              </kbd>
            </button>

            <DropdownMenu.Root>
              <DropdownMenu.Trigger asChild>
                <button
                  type="button"
                  className="inline-flex h-7 items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-2 text-xs font-medium text-zinc-700 hover:bg-zinc-50 cursor-pointer"
                >
                  <Globe className="h-3.5 w-3.5 text-zinc-400" />
                  <span className="font-mono">{activeLocale}</span>
                  <ChevronDown className="h-3 w-3 text-zinc-400" />
                </button>
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content
                  align="end"
                  sideOffset={6}
                  className="z-50 min-w-36 rounded-md border border-zinc-200 bg-white p-1 shadow-md"
                >
                  {["en", "fr", "de", "bn"].map((loc) => (
                    <DropdownMenu.Item
                      key={loc}
                      onSelect={() => setActiveLocale(loc)}
                      className="flex cursor-pointer items-center justify-between rounded px-2 py-1.5 text-xs text-zinc-700 outline-none hover:bg-indigo-50 hover:text-indigo-900"
                    >
                      <span>Locale</span>
                      <span className="font-mono font-medium">{loc}</span>
                    </DropdownMenu.Item>
                  ))}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>

            <Badge tone={isOnline ? "emerald" : readinessQuery.isLoading ? "neutral" : "amber"}>
              <span
                className={cn(
                  "h-1.5 w-1.5 rounded-full",
                  isOnline
                    ? "bg-emerald-600"
                    : readinessQuery.isLoading
                      ? "bg-zinc-400"
                      : "bg-amber-600",
                )}
              />
              {isOnline
                ? `API ready (${readinessQuery.data.cacheBackend})`
                : readinessQuery.isLoading
                  ? "Checking API"
                  : "API offline"}
            </Badge>
          </div>
        </header>

        {/* First-admin setup banner when database has zero admin users */}
        {initQuery.data && !initQuery.data.hasAdmin && (
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-indigo-200 bg-indigo-50/70 px-5 py-2 text-xs text-indigo-950">
            <div>
              <span className="font-medium">No administrator account configured yet.</span>{" "}
              <span className="text-indigo-900/80">
                Complete initial Super Admin setup to lock down admin APIs in Phase 1.
              </span>
            </div>
            <Button size="sm" asChild>
              <Link to="/onboarding">Create administrator</Link>
            </Button>
          </div>
        )}

        <main className="flex-1 p-6">
          <div className="mx-auto max-w-6xl">
            <Outlet />
          </div>
        </main>
      </div>
    </div>
  );
}
