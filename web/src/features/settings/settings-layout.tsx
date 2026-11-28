import {
  ArrowLeftRight,
  Eye,
  GitPullRequest,
  Globe,
  KeyRound,
  Mail,
  Rocket,
  ScrollText,
  Settings,
  ShieldCheck,
  UserCog,
  Webhook,
} from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import { cn } from "@/lib/utils";

const SETTINGS_SECTIONS = [
  {
    title: "Global settings",
    items: [
      { label: "Overview", to: "/settings/general", icon: Settings },
      { label: "API Tokens", to: "/settings/api-tokens", icon: KeyRound },
      { label: "Transfer Tokens", to: "/settings/admin-tokens", icon: KeyRound },
      { label: "Internationalization", to: "/settings/i18n", icon: Globe },
      { label: "Media Library", to: "/media-library", icon: Eye },
      { label: "Webhooks", to: "/settings/webhooks", icon: Webhook },
    ],
  },
  {
    title: "Administration panel",
    items: [
      { label: "Roles & Users", to: "/users", icon: ShieldCheck },
      { label: "Review Workflows", to: "/settings/workflows", icon: GitPullRequest },
      { label: "Releases", to: "/settings/releases", icon: Rocket },
      { label: "Audit Logs", to: "/settings/audit-logs", icon: ScrollText },
    ],
  },
  {
    title: "Plugins & delivery",
    items: [
      { label: "Live Preview", to: "/settings/preview", icon: Eye },
      { label: "Email Plugin", to: "/settings/email", icon: Mail },
      { label: "Data Transfer", to: "/settings/data-transfer", icon: ArrowLeftRight },
      { label: "Profile", to: "/account", icon: UserCog },
    ],
  },
];

export function SettingsLayout() {
  return (
    <div className="flex min-h-[calc(100vh-3rem)] -m-6 bg-[#f6f6f9]">
      <aside className="w-56 shrink-0 border-r border-[#eaeaef] bg-white py-5 px-3">
        <div className="mb-4 px-2.5">
          <h2 className="text-base font-bold tracking-tight text-[#32324d]">Settings</h2>
        </div>

        <nav className="space-y-5">
          {SETTINGS_SECTIONS.map((group) => (
            <div key={group.title} className="space-y-0.5">
              <div className="px-2.5 pb-1 text-[11px] font-bold uppercase tracking-wider text-[#666687]">
                {group.title}
              </div>
              {group.items.map((item) => {
                const Icon = item.icon;
                return (
                  <NavLink
                    key={item.to}
                    to={item.to}
                    className={({ isActive }) =>
                      cn(
                        "flex items-center gap-2.5 rounded px-2.5 py-1.5 text-xs font-medium transition-colors",
                        isActive
                          ? "bg-[#f0f0ff] font-semibold text-[#4945ff]"
                          : "text-[#32324d] hover:bg-[#f6f6f9]",
                      )
                    }
                  >
                    <Icon className="h-3.5 w-3.5 shrink-0" />
                    <span className="truncate">{item.label}</span>
                  </NavLink>
                );
              })}
            </div>
          ))}
        </nav>
      </aside>

      <div className="min-w-0 flex-1 p-6 lg:p-8">
        <div className="mx-auto max-w-5xl">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
