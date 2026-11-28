import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  description: string;
  action?: ReactNode;
}

export function EmptyState({ icon: Icon, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center rounded-md border border-dashed border-zinc-300 bg-white px-6 py-12 text-center">
      {Icon && (
        <div className="mb-3 flex h-9 w-9 items-center justify-center rounded-md border border-zinc-200 bg-zinc-50 text-zinc-500">
          <Icon className="h-4 w-4" />
        </div>
      )}
      <h3 className="text-sm font-semibold text-zinc-900">{title}</h3>
      <p className="mt-1 max-w-md text-xs text-zinc-600">{description}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
