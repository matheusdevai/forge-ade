/**
 * Official FORGE brand assets.
 *
 * Files are loaded from /assets/brand at build time. If an official file is
 * missing, we do NOT substitute a generic image: the logo falls back to a
 * plain typographic wordmark and the mascot is simply not rendered.
 *
 *   assets/brand/forge-logo.png            symbol + wordmark (official)
 *   assets/brand/forge-mascot.png          mascot (official)
 *   assets/brand/forge-symbol.png          optional: symbol-only crop for compact spots
 *   assets/brand/mascot/<state>.png        optional: official per-state artwork
 */
import { cn } from "@/lib/utils";
import { useSettings } from "@/stores/settings";

const files = import.meta.glob("../../assets/brand/**/*.{png,webp,svg}", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

function find(relative: string): string | null {
  const key = Object.keys(files).find((k) => k.replace(/\.(png|webp|svg)$/, "").endsWith(`/assets/brand/${relative}`));
  return key ? (files[key] ?? null) : null;
}

export const MASCOT_STATES = ["online", "thinking", "working", "waiting", "success", "error", "sleeping"] as const;
export type MascotState = (typeof MASCOT_STATES)[number];

export const brand = {
  logo: find("forge-logo"),
  symbol: find("forge-symbol"),
  mascot: find("forge-mascot"),
  mascotStates: Object.fromEntries(MASCOT_STATES.map((s) => [s, find(`mascot/${s}`)])) as Record<MascotState, string | null>,
};

export const brandStatus = [
  { file: "assets/brand/forge-logo.png", present: !!brand.logo, required: true },
  { file: "assets/brand/forge-mascot.png", present: !!brand.mascot, required: true },
  { file: "assets/brand/forge-symbol.png", present: !!brand.symbol, required: false },
  ...MASCOT_STATES.map((s) => ({ file: `assets/brand/mascot/${s}.png`, present: !!brand.mascotStates[s], required: false })),
];

/** Typographic wordmark — used only until the official logo file is provided. */
function Wordmark({ className }: { className?: string }) {
  return <span className={cn("font-semibold tracking-[0.32em] text-fg", className)}>FORGE</span>;
}

export function Logo({ variant = "full", className }: { variant?: "full" | "compact"; className?: string }) {
  const src = variant === "compact" ? (brand.symbol ?? brand.logo) : brand.logo;
  if (!src) return <Wordmark className={cn(variant === "compact" ? "text-[11px]" : "text-sm", className)} />;
  return <img src={src} alt="FORGE" draggable={false} className={cn("object-contain", className)} />;
}

const stateFx: Record<MascotState, string> = {
  online: "[--glow:#579bff66] animate-glow",
  thinking: "[--glow:#7b8dff88] animate-glow",
  working: "[--glow:#579bffaa] animate-glow-fast",
  waiting: "opacity-80 saturate-75",
  success: "[--glow:#35c98a88] animate-glow",
  error: "[--glow:#f16f7899] animate-glow-fast",
  sleeping: "opacity-50 grayscale",
};

/**
 * The FORGE mascot. Per-state official artwork is used when available;
 * otherwise the base artwork gets a subtle state treatment (glow/tint) —
 * we never redraw or alter the character itself.
 */
export function Mascot({ state = "online", size = 96, className }: { state?: MascotState; size?: number; className?: string }) {
  const enabled = useSettings((s) => s.values["ui.mascot"]);
  const specific = brand.mascotStates[state];
  const src = specific ?? brand.mascot;
  if (!enabled || !src) return null;
  return (
    <img
      src={src}
      alt={`Mascote FORGE — ${state}`}
      draggable={false}
      width={size}
      height={size}
      style={{ width: size, height: size }}
      className={cn("object-contain select-none", !specific && stateFx[state], className)}
    />
  );
}

export const hasMascot = !!brand.mascot;
