import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "neutral" | "indigo" | "emerald" | "amber" | "rose" | "mono";
}

export function Badge({ className, tone = "neutral", ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium leading-none border",
        tone === "neutral" && "bg-zinc-100 text-zinc-700 border-zinc-200",
        tone === "indigo" && "bg-indigo-50 text-indigo-700 border-indigo-200",
        tone === "emerald" && "bg-emerald-50 text-emerald-700 border-emerald-200",
        tone === "amber" && "bg-amber-50 text-amber-800 border-amber-200",
        tone === "rose" && "bg-rose-50 text-rose-700 border-rose-200",
        tone === "mono" && "bg-zinc-100 text-zinc-800 border-zinc-200 font-mono text-[11px]",
        className,
      )}
      {...props}
    />
  );
}
