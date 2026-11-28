import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { type ComponentPropsWithoutRef, forwardRef } from "react";
import { cn } from "@/lib/utils";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export const DialogContent = forwardRef<
  HTMLDivElement,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Content> & { title: string; description?: string }
>(({ className, children, title, description, ...props }, ref) => (
  <DialogPrimitive.Portal>
    <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-zinc-950/35 backdrop-blur-[1px]" />
    <DialogPrimitive.Content
      ref={ref}
      aria-describedby={description ? undefined : undefined}
      className={cn(
        "fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-md border border-zinc-200 bg-white p-5 shadow-lg focus:outline-none",
        className,
      )}
      {...props}
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <DialogPrimitive.Title className="text-sm font-semibold text-zinc-900">
            {title}
          </DialogPrimitive.Title>
          {description && (
            <DialogPrimitive.Description className="mt-0.5 text-xs text-zinc-600">
              {description}
            </DialogPrimitive.Description>
          )}
        </div>
        <DialogPrimitive.Close className="rounded p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-700 cursor-pointer">
          <X className="h-4 w-4" />
        </DialogPrimitive.Close>
      </div>
      {children}
    </DialogPrimitive.Content>
  </DialogPrimitive.Portal>
));
DialogContent.displayName = "DialogContent";
