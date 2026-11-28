import {
  Blocks,
  Compass,
  ExternalLink,
  Globe,
  KeyRound,
  Mail,
  ShieldCheck,
  Sparkles,
  Webhook,
} from "lucide-react";
import { Link } from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

const BUILTIN_PLUGINS = [
  {
    name: "Content-Type Builder",
    description: "Model Collection Types, Single Types, and reusable Components with live SQLite DDL.",
    category: "Core",
    status: "Installed",
    icon: Blocks,
    to: "/content-type-builder",
  },
  {
    name: "Internationalization (i18n)",
    description: "Author localized entries per locale with independent Draft & Publish workflows.",
    category: "Plugin",
    status: "Installed",
    icon: Globe,
    to: "/settings/i18n",
  },
  {
    name: "Users & Permissions",
    description: "End-user JWT authentication, Public/Authenticated roles, and granular RBAC.",
    category: "Plugin",
    status: "Installed",
    icon: ShieldCheck,
    to: "/users",
  },
  {
    name: "Email Delivery",
    description: "Transactional email templates, SMTP delivery transport, and outbox inspection.",
    category: "Provider",
    status: "Installed",
    icon: Mail,
    to: "/settings/email",
  },
  {
    name: "Webhooks & Event Bus",
    description: "HMAC-SHA256 signed webhook deliveries on entry and media lifecycle events.",
    category: "Integration",
    status: "Installed",
    icon: Webhook,
    to: "/settings/webhooks",
  },
  {
    name: "API & Transfer Tokens",
    description: "Scoped read-only, full-access, and custom permission API keys.",
    category: "Security",
    status: "Installed",
    icon: KeyRound,
    to: "/settings/api-tokens",
  },
];

export function MarketplacePage() {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold tracking-tight text-[#32324d]">Marketplace</h1>
          <p className="mt-1 text-xs text-[#666687]">
            Built-in Rust plugins and Strapi v5-compatible capabilities compiled directly into RAPI.
          </p>
        </div>
        <Button variant="secondary" size="sm" asChild>
          <a href="/openapi.json" target="_blank" rel="noreferrer">
            OpenAPI 3.1 Spec
            <ExternalLink className="h-3.5 w-3.5" />
          </a>
        </Button>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        {BUILTIN_PLUGINS.map((plugin) => {
          const Icon = plugin.icon;
          return (
            <div
              key={plugin.name}
              className="flex flex-col justify-between rounded-[4px] border border-[#eaeaef] bg-white p-5 shadow-[0_1px_4px_rgba(33,33,52,0.06)]"
            >
              <div>
                <div className="flex items-start justify-between gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-md bg-[#f0f0ff] text-[#4945ff]">
                    <Icon className="h-5 w-5" />
                  </div>
                  <Badge tone="emerald">{plugin.status}</Badge>
                </div>
                <h2 className="mt-4 text-sm font-semibold text-[#32324d]">{plugin.name}</h2>
                <p className="mt-1.5 text-xs leading-relaxed text-[#666687]">
                  {plugin.description}
                </p>
              </div>

              <div className="mt-5 flex items-center justify-between border-t border-[#f0f0f5] pt-3">
                <span className="inline-flex items-center gap-1 text-[11px] font-medium text-[#8e8ea9]">
                  <Sparkles className="h-3 w-3 text-[#4945ff]" />
                  {plugin.category}
                </span>
                <Link
                  to={plugin.to}
                  className="text-xs font-semibold text-[#4945ff] hover:underline"
                >
                  Configure →
                </Link>
              </div>
            </div>
          );
        })}
      </div>

      <div className="flex items-center justify-between rounded-[4px] border border-[#eaeaef] bg-white px-5 py-4 text-xs text-[#666687]">
        <div className="flex items-center gap-2.5">
          <Compass className="h-4 w-4 text-[#4945ff]" />
          <span>
            Zero external runtime plugins required — GraphQL v5, FTS5 search, and image processing run natively in Rust.
          </span>
        </div>
        <Link to="/" className="font-semibold text-[#4945ff] hover:underline">
          Back to Home
        </Link>
      </div>
    </div>
  );
}
