import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronDown,
  Cloud,
  Feather,
  Globe,
  Home,
  Image as ImageIcon,
  KeyRound,
  LayoutGrid,
  LogOut,
  Rocket,
  Search,
  Settings,
  ShoppingCart,
  User,
} from "lucide-react";
import { useEffect } from "react";
import { Link, Navigate, NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useUiStore } from "@/app/store";
import { Badge } from "@/components/ui/badge";
import { CommandDialog } from "@/components/ui/command-dialog";
import { Tooltip } from "@/components/ui/tooltip";
import { useAuthStore } from "@/features/auth/auth-store";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";

const RAIL_ITEMS = [
  {
    label: "Home",
    to: "/",
    icon: Home,
    end: true,
    matchPrefix: undefined,
  },
  {
    label: "Content Manager",
    to: "/content-manager",
    icon: Feather,
    end: false,
    matchPrefix: "/content-manager",
  },
  {
    label: "Media Library",
    to: "/media-library",
    icon: ImageIcon,
    end: false,
    matchPrefix: "/media-library",
  },
  {
    label: "Content-Type Builder",
    to: "/content-type-builder",
    icon: LayoutGrid,
    end: false,
    matchPrefix: "/content-type-builder",
  },
  {
    label: "Releases & Deploy",
    to: "/settings/releases",
    icon: Cloud,
    end: false,
    matchPrefix: "/settings/releases",
  },
  {
    label: "Marketplace & Plugins",
    to: "/marketplace",
    icon: ShoppingCart,
    end: false,
    matchPrefix: "/marketplace",
  },
  {
    label: "Settings",
    to: "/settings/api-tokens",
    icon: Settings,
    end: false,
    matchPrefix: "/settings",
    excludePrefixes: ["/settings/releases"],
  },
];

function getInitials(firstname?: string, lastname?: string, email?: string): string {
  const f = (firstname ?? "").trim();
  const l = (lastname ?? "").trim();
  if (f && l) {
    return `${f[0]}${l[0]}`.toUpperCase();
  }
  if (f.length >= 2) {
    return f.slice(0, 2).toUpperCase();
  }
  if (f.length === 1 && email) {
    return `${f[0]}${email[0]}`.toUpperCase();
  }
  if (email && email.length >= 2) {
    return email.slice(0, 2).toUpperCase();
  }
  return "AD";
}

export function AppLayout() {
  const { activeLocale, setActiveLocale, setCommandOpen } = useUiStore();
  const { token, refreshToken, user, clearSession } = useAuthStore();
  const location = useLocation();
  const navigate = useNavigate();

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

  const localesQuery = useQuery({
    queryKey: ["locales"],
    queryFn: api.listLocales,
    enabled: Boolean(token),
  });

  const configuredLocales = localesQuery.data ?? [];

  useEffect(() => {
    if (configuredLocales.length > 0 && !configuredLocales.some((l) => l.code === activeLocale)) {
      const def = configuredLocales.find((l) => l.isDefault) ?? configuredLocales[0];
      if (def) setActiveLocale(def.code);
    }
  }, [configuredLocales, activeLocale, setActiveLocale]);

  if (initQuery.isSuccess && !initQuery.data.hasAdmin) {
    return <Navigate to="/onboarding" replace />;
  }

  if (!token) {
    return <Navigate to="/login" replace />;
  }

  const handleLogout = async () => {
    if (refreshToken) {
      try {
        await api.logout(refreshToken);
      } catch {
        // ignore network errors on logout
      }
    }
    clearSession();
    navigate("/login", { replace: true });
  };

  const isOnline = readinessQuery.isSuccess && readinessQuery.data.status === "ready";
  const isHomeRoute = location.pathname === "/";
  const isContentManagerRoute = location.pathname.startsWith("/content-manager");
  const hideTopHeader = isHomeRoute || isContentManagerRoute;
  const initials = getInitials(user?.firstname, user?.lastname, user?.email);

  return (
    <div className="flex min-h-screen bg-[#f6f6f9] text-[#32324d]">
      <CommandDialog />

      {/* Left Strapi-style Slim Icon Navigation Rail */}
      <aside className="sticky top-0 z-30 flex h-screen w-[56px] shrink-0 flex-col items-center justify-between border-r border-[#eaeaef] bg-white py-3 select-none">
        <div className="flex w-full flex-col items-center">
          {/* Brand Logo */}
          <Tooltip content={`RAPI v${initQuery.data?.version ?? "0.1.0"} — Quick Jump (⌘K)`}>
            <Link
              to="/"
              className="mb-3 flex h-8 w-8 items-center justify-center rounded-[6px] bg-[#4945ff] text-white shadow-xs transition-transform hover:scale-[1.03]"
              aria-label="Home"
            >
              {/* Geometric Strapi/RAPI corner mark */}
              <svg
                viewBox="0 0 24 24"
                fill="none"
                className="h-4 w-4"
                aria-hidden="true"
              >
                <path
                  d="M6 5.5C6 4.94772 6.44772 4.5 7 4.5H18.5C19.0523 4.5 19.5 4.94772 19.5 5.5V17C19.5 17.5523 19.0523 18 18.5 18H13.5V11.5C13.5 10.9477 13.0523 10.5 12.5 10.5H6V5.5Z"
                  fill="currentColor"
                />
                <path
                  d="M6 12.5H11C11.2761 12.5 11.5 12.7239 11.5 13V18H6.5C6.22386 18 6 17.7761 6 17.5V12.5Z"
                  fill="currentColor"
                  fillOpacity="0.55"
                />
              </svg>
            </Link>
          </Tooltip>

          <div className="mb-2.5 h-px w-7 bg-[#eaeaef]" />

          {/* Primary Icon Stack */}
          <nav className="flex w-full flex-col items-center gap-1.5 px-2">
            {RAIL_ITEMS.map((item) => {
              const Icon = item.icon;
              const excluded = item.excludePrefixes?.some((p) =>
                location.pathname.startsWith(p),
              );
              const isSettingsUsersAlias =
                item.matchPrefix === "/settings" &&
                (location.pathname.startsWith("/users") ||
                  location.pathname.startsWith("/roles") ||
                  location.pathname.startsWith("/account"));
              const active = item.end
                ? location.pathname === item.to
                : !excluded &&
                  ((item.matchPrefix ? location.pathname.startsWith(item.matchPrefix) : false) ||
                    isSettingsUsersAlias);

              return (
                <Tooltip key={item.label} content={item.label}>
                  <NavLink
                    to={item.to}
                    end={item.end}
                    aria-label={item.label}
                    className={cn(
                      "flex h-9 w-9 items-center justify-center rounded-[6px] transition-colors",
                      active
                        ? "bg-[#f0f0ff] text-[#4945ff]"
                        : "text-[#666687] hover:bg-[#f6f6f9] hover:text-[#32324d]",
                    )}
                  >
                    <Icon className="h-[18px] w-[18px]" strokeWidth={active ? 2.2 : 1.85} />
                  </NavLink>
                </Tooltip>
              );
            })}
          </nav>
        </div>

        {/* Bottom Avatar Menu */}
        <div className="flex flex-col items-center gap-2">
          <Tooltip content="Search & Jump (⌘K)">
            <button
              type="button"
              onClick={() => setCommandOpen(true)}
              aria-label="Open command menu"
              className="flex h-8 w-8 items-center justify-center rounded-[6px] text-[#8e8ea9] hover:bg-[#f6f6f9] hover:text-[#32324d] cursor-pointer"
            >
              <Search className="h-4 w-4" />
            </button>
          </Tooltip>

          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label="Account menu"
                className="flex h-8 w-8 items-center justify-center rounded-full bg-[#4945ff] text-[11px] font-bold tracking-tight text-white shadow-xs ring-2 ring-transparent transition hover:ring-[#d9d8ff] focus:outline-none cursor-pointer"
              >
                {initials}
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                side="right"
                align="end"
                sideOffset={10}
                className="z-50 w-56 rounded-md border border-[#eaeaef] bg-white p-1.5 shadow-lg"
              >
                {user && (
                  <div className="border-b border-[#f0f0f5] px-2.5 py-2">
                    <div className="truncate text-xs font-semibold text-[#32324d]">
                      {user.firstname} {user.lastname}
                    </div>
                    <div className="truncate text-[11px] text-[#666687]">{user.email}</div>
                  </div>
                )}

                <div className="py-1">
                  <DropdownMenu.Item
                    onSelect={() => navigate("/account")}
                    className="flex cursor-pointer items-center gap-2 rounded px-2.5 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                  >
                    <User className="h-3.5 w-3.5" />
                    <span>Profile settings</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    onSelect={() => navigate("/settings/api-tokens")}
                    className="flex cursor-pointer items-center gap-2 rounded px-2.5 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                  >
                    <KeyRound className="h-3.5 w-3.5" />
                    <span>API Tokens</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    onSelect={() => navigate("/settings/releases")}
                    className="flex cursor-pointer items-center gap-2 rounded px-2.5 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                  >
                    <Rocket className="h-3.5 w-3.5" />
                    <span>Releases</span>
                  </DropdownMenu.Item>
                </div>

                <div className="border-t border-[#f0f0f5] pt-1">
                  <DropdownMenu.Item
                    onSelect={handleLogout}
                    className="flex cursor-pointer items-center gap-2 rounded px-2.5 py-1.5 text-xs font-medium text-[#d02b20] outline-none hover:bg-[#fcecea]"
                  >
                    <LogOut className="h-3.5 w-3.5" />
                    <span>Log out</span>
                  </DropdownMenu.Item>
                </div>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>
      </aside>

      {/* Main workspace column */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Show slim workspace utility header on sub-pages; keep Home and Content Manager canvases uninterrupted like Strapi v5 */}
        {!hideTopHeader && (
          <header className="sticky top-0 z-20 flex h-12 items-center justify-between border-b border-[#eaeaef] bg-white/95 px-6 backdrop-blur-xs">
            <div className="flex items-center gap-2 text-xs text-[#666687]">
              <Link to="/" className="font-medium text-[#32324d] hover:text-[#4945ff]">
                Administration
              </Link>
              <span>/</span>
              <span className="font-mono text-[11px] text-[#666687]">{location.pathname}</span>
            </div>

            <div className="flex items-center gap-2.5">
              <button
                type="button"
                onClick={() => setCommandOpen(true)}
                className="flex h-7 items-center gap-2 rounded border border-[#dcdce4] bg-[#f6f6f9] px-2.5 text-xs text-[#666687] hover:border-[#c0c0cf] hover:text-[#32324d] cursor-pointer"
              >
                <Search className="h-3.5 w-3.5" />
                <span className="hidden sm:inline">Jump to...</span>
                <kbd className="rounded border border-[#dcdce4] bg-white px-1 font-mono text-[10px] text-[#666687]">
                  ⌘K
                </kbd>
              </button>

              <DropdownMenu.Root>
                <DropdownMenu.Trigger asChild>
                  <button
                    type="button"
                    className="inline-flex h-7 items-center gap-1.5 rounded border border-[#dcdce4] bg-white px-2 text-xs font-medium text-[#32324d] hover:bg-[#f6f6f9] cursor-pointer"
                  >
                    <Globe className="h-3.5 w-3.5 text-[#8e8ea9]" />
                    <span className="font-mono">{activeLocale}</span>
                    <ChevronDown className="h-3 w-3 text-[#8e8ea9]" />
                  </button>
                </DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content
                    align="end"
                    sideOffset={6}
                    className="z-50 min-w-44 rounded-md border border-[#eaeaef] bg-white p-1 shadow-md"
                  >
                    {(configuredLocales.length > 0
                      ? configuredLocales
                      : [{ id: 1, code: "en", name: "English (en)", isDefault: true }]
                    ).map((loc) => (
                      <DropdownMenu.Item
                        key={loc.code}
                        onSelect={() => setActiveLocale(loc.code)}
                        className="flex cursor-pointer items-center justify-between gap-3 rounded px-2 py-1.5 text-xs text-[#32324d] outline-none hover:bg-[#f0f0ff] hover:text-[#4945ff]"
                      >
                        <span className="truncate">{loc.name}</span>
                        <span className="font-mono font-medium">{loc.code}</span>
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
        )}

        <main
          className={cn(
            "flex-1",
            isContentManagerRoute
              ? "p-0"
              : isHomeRoute
                ? "px-8 py-9 lg:px-12"
                : "p-6",
          )}
        >
          <div
            className={cn(
              isContentManagerRoute
                ? "w-full"
                : isHomeRoute
                  ? "mx-auto max-w-[1360px]"
                  : "mx-auto max-w-6xl",
            )}
          >
            <Outlet />
          </div>
        </main>
      </div>
    </div>
  );
}
