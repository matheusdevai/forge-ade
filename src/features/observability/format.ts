import type { Reported } from "@/lib/ipc";

export function fmtInt(n: number): string {
  return new Intl.NumberFormat("pt-BR").format(n);
}

export function fmtCompact(n: number): string {
  return new Intl.NumberFormat("pt-BR", { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

export function fmtUsd(n: number): string {
  return n < 0.01 && n > 0 ? `US$ ${n.toFixed(4)}` : `US$ ${n.toFixed(2)}`;
}

export function fmtMs(ms: number | null): string {
  if (ms === null) return "—";
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  return `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
}

export function fmtKb(kb: number | null): string {
  if (kb === null) return "—";
  return kb > 1024 * 1024 ? `${(kb / 1024 / 1024).toFixed(1)} GB` : kb > 1024 ? `${(kb / 1024).toFixed(0)} MB` : `${kb} KB`;
}

/** "informado em X de Y" — never presents unknown data as zero. */
export function coverage(r: Reported<unknown>): string {
  const total = r.reportedRuns + r.unreportedRuns;
  if (total === 0) return "sem execuções finalizadas";
  if (r.reportedRuns === 0) return "não informado pelos CLIs";
  if (r.unreportedRuns === 0) return `informado em todas as ${total}`;
  return `informado em ${r.reportedRuns} de ${total}`;
}

/** The value to display: "—" when no run reported it (never a fake 0). */
export function reportedValue(r: Reported<number>, f: (n: number) => string): string {
  return r.value === null || r.reportedRuns === 0 ? "—" : f(r.value);
}
