import { Slot } from "@radix-ui/react-slot";
import { type ButtonHTMLAttributes, forwardRef } from "react";
import { cn } from "@/lib/utils";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md";
  asChild?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "primary", size = "md", asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        ref={ref}
        className={cn(
          "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 disabled:pointer-events-none disabled:opacity-50 cursor-pointer",
          variant === "primary" &&
            "bg-indigo-700 text-white hover:bg-indigo-800 border border-indigo-700",
          variant === "secondary" &&
            "bg-white text-zinc-800 border border-zinc-300 hover:bg-zinc-50",
          variant === "ghost" &&
            "bg-transparent text-zinc-600 hover:bg-zinc-200/60 hover:text-zinc-900",
          variant === "danger" &&
            "bg-rose-600 text-white border border-rose-600 hover:bg-rose-700",
          size === "sm" && "h-7 px-2.5 text-xs",
          size === "md" && "h-8 px-3 text-xs",
          className,
        )}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";
