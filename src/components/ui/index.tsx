/**
 * FORGE UI primitives (shadcn/ui-style: Radix + cva + Tailwind tokens).
 */
import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { cva, type VariantProps } from "class-variance-authority";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

// ── Button ────────────────────────────────────────────────────────────────
export const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium transition-colors disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-3.5 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-accent text-[#05080d] hover:bg-[#6aa7ff] shadow-[0_0_0_1px_#579bff,0_4px_18px_-6px_#579bff99]",
        secondary: "bg-elevated text-fg border border-border hover:border-border-strong hover:bg-[#212733]",
        ghost: "text-muted hover:text-fg hover:bg-elevated",
        outline: "border border-border text-fg hover:bg-elevated",
        danger: "bg-error/15 text-error border border-error/30 hover:bg-error/25",
      },
      size: {
        sm: "h-7 px-2.5 text-xs",
        md: "h-8 px-3 text-[13px]",
        lg: "h-10 px-4 text-sm tracking-wide",
        icon: "h-7 w-7",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof buttonVariants>;

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(({ className, variant, size, ...props }, ref) => (
  <button ref={ref} className={cn(buttonVariants({ variant, size }), className)} {...props} />
));
Button.displayName = "Button";

// ── Badge ─────────────────────────────────────────────────────────────────
const badgeVariants = cva("inline-flex items-center gap-1 rounded px-1.5 py-px text-[10.5px] font-medium leading-4", {
  variants: {
    tone: {
      neutral: "bg-elevated text-muted border border-border",
      accent: "bg-accent/12 text-accent border border-accent/25",
      success: "bg-success/12 text-success border border-success/25",
      warning: "bg-warning/12 text-warning border border-warning/25",
      error: "bg-error/12 text-error border border-error/25",
    },
  },
  defaultVariants: { tone: "neutral" },
});
export function Badge({
  className,
  tone,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ tone }), className)} {...props} />;
}

export function Kbd({ className, ...props }: React.HTMLAttributes<HTMLElement>) {
  return (
    <kbd
      className={cn(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center rounded border border-border bg-bg-2 px-1 font-sans text-[10.5px] text-muted",
        className,
      )}
      {...props}
    />
  );
}

export function StatusDot({ tone = "neutral", pulse }: { tone?: "neutral" | "success" | "warning" | "error" | "accent"; pulse?: boolean }) {
  const color = {
    neutral: "bg-subtle",
    success: "bg-success",
    warning: "bg-warning",
    error: "bg-error",
    accent: "bg-accent",
  }[tone];
  return (
    <span className="relative inline-flex size-2 shrink-0">
      {pulse && <span className={cn("absolute inset-0 animate-ping rounded-full opacity-60", color)} />}
      <span className={cn("relative inline-flex size-2 rounded-full", color)} />
    </span>
  );
}

// ── Card ──────────────────────────────────────────────────────────────────
export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-xl border border-border bg-surface", className)} {...props} />;
}

export function CardHeader({
  title,
  icon,
  action,
  className,
}: {
  title: React.ReactNode;
  icon?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex h-10 items-center gap-2 border-b border-border px-4", className)}>
      {icon && <span className="text-subtle [&_svg]:size-3.5">{icon}</span>}
      <h3 className="text-xs font-medium uppercase tracking-[0.08em] text-muted">{title}</h3>
      <div className="ml-auto flex items-center gap-1">{action}</div>
    </div>
  );
}

// ── Inputs ────────────────────────────────────────────────────────────────
export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "h-9 w-full rounded-md border border-border bg-bg-2 px-3 text-[13px] text-fg placeholder:text-subtle focus:border-accent/60 focus:outline-none",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";

export const Textarea = React.forwardRef<HTMLTextAreaElement, React.TextareaHTMLAttributes<HTMLTextAreaElement>>(
  ({ className, ...props }, ref) => (
    <textarea
      ref={ref}
      className={cn(
        "w-full resize-none rounded-md border border-border bg-bg-2 px-3 py-2 text-[13px] leading-relaxed text-fg placeholder:text-subtle focus:border-accent/60 focus:outline-none",
        className,
      )}
      {...props}
    />
  ),
);
Textarea.displayName = "Textarea";

export function Label({ className, ...props }: React.LabelHTMLAttributes<HTMLLabelElement>) {
  return <label className={cn("mb-1.5 block text-xs font-medium text-muted", className)} {...props} />;
}

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  label,
}: {
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "relative h-5 w-9 shrink-0 rounded-full border transition-colors disabled:opacity-40",
        checked ? "border-accent bg-accent" : "border-border bg-elevated",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 size-3.5 rounded-full bg-fg shadow transition-transform",
          checked ? "translate-x-[18px]" : "translate-x-0.5",
        )}
      />
    </button>
  );
}

// ── Dialog ────────────────────────────────────────────────────────────────
export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export function DialogContent({
  className,
  children,
  title,
  description,
  hideClose,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & { title: string; description?: string; hideClose?: boolean }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-[2px]" />
      <DialogPrimitive.Content
        className={cn(
          "fixed top-1/2 left-1/2 z-50 w-[min(560px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-surface shadow-2xl shadow-black/60 focus:outline-none",
          className,
        )}
        {...props}
      >
        <div className="flex items-start gap-3 border-b border-border px-5 py-4">
          <div className="min-w-0 flex-1">
            <DialogPrimitive.Title className="text-[15px] font-semibold text-fg">{title}</DialogPrimitive.Title>
            {description ? (
              <DialogPrimitive.Description className="mt-1 text-xs text-muted">{description}</DialogPrimitive.Description>
            ) : (
              <DialogPrimitive.Description className="sr-only">{title}</DialogPrimitive.Description>
            )}
          </div>
          {!hideClose && (
            <DialogPrimitive.Close className={buttonVariants({ variant: "ghost", size: "icon" })} aria-label="Fechar">
              <X />
            </DialogPrimitive.Close>
          )}
        </div>
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

// ── Tooltip ───────────────────────────────────────────────────────────────
export const TooltipProvider = TooltipPrimitive.Provider;

export function Tooltip({ content, children, side = "bottom" }: { content: React.ReactNode; children: React.ReactNode; side?: "top" | "bottom" | "left" | "right" }) {
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="z-[60] max-w-xs rounded-md border border-border bg-elevated px-2 py-1 text-[11.5px] text-fg shadow-lg"
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

// ── Dropdown ──────────────────────────────────────────────────────────────
export const DropdownMenu = DropdownPrimitive.Root;
export const DropdownMenuTrigger = DropdownPrimitive.Trigger;

export function DropdownMenuContent({ className, ...props }: React.ComponentProps<typeof DropdownPrimitive.Content>) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.Content
        sideOffset={4}
        className={cn("z-50 min-w-[200px] rounded-lg border border-border bg-elevated p-1 shadow-xl shadow-black/50", className)}
        {...props}
      />
    </DropdownPrimitive.Portal>
  );
}

export function DropdownMenuItem({ className, ...props }: React.ComponentProps<typeof DropdownPrimitive.Item>) {
  return (
    <DropdownPrimitive.Item
      className={cn(
        "flex h-7 cursor-default items-center gap-2 rounded px-2 text-[12.5px] text-fg outline-none data-[disabled]:text-subtle data-[highlighted]:bg-accent/15 [&_svg]:size-3.5 [&_svg]:text-muted",
        className,
      )}
      {...props}
    />
  );
}

export function DropdownMenuLabel({ className, ...props }: React.ComponentProps<typeof DropdownPrimitive.Label>) {
  return <DropdownPrimitive.Label className={cn("px-2 pt-1.5 pb-1 text-[10.5px] uppercase tracking-wider text-subtle", className)} {...props} />;
}

export function DropdownMenuSeparator() {
  return <DropdownPrimitive.Separator className="my-1 h-px bg-border" />;
}

// ── Empty state ───────────────────────────────────────────────────────────
export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: {
  icon?: React.ReactNode;
  title: string;
  description?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center justify-center px-6 py-8 text-center", className)}>
      {icon && <div className="mb-3 text-subtle [&_svg]:size-5">{icon}</div>}
      <p className="text-[13px] font-medium text-fg">{title}</p>
      {description && <p className="mt-1 max-w-sm text-xs leading-relaxed text-muted">{description}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
