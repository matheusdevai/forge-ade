import { useState } from "react";
import { Table2 } from "lucide-react";
import { cn } from "@/lib/utils";

export type Bar = { key: string; label: string; value: number | null; tooltip: string };

/**
 * Single-series bar chart (one measure per chart — never a dual axis).
 * Thin bars with a rounded top anchored to the baseline, 2px gaps, recessive
 * grid, per-bar hover tooltip, and a table view. `null` = not reported
 * (drawn as an empty slot, never as zero).
 */
export function BarChart({ title, bars, format, height = 120 }: { title: string; bars: Bar[]; format: (n: number) => string; height?: number }) {
  const [hover, setHover] = useState<number | null>(null);
  const [table, setTable] = useState(false);
  const max = Math.max(0, ...bars.map((b) => b.value ?? 0));
  const n = Math.max(1, bars.length);
  const w = 100 / n;
  const labelEvery = Math.ceil(n / 8);
  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        <h3 className="text-[12.5px] font-medium text-fg">{title}</h3>
        <span className="text-[11px] text-subtle">máx. {max > 0 ? format(max) : "—"}</span>
        <button className={cn("ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px]", table ? "bg-elevated text-fg" : "text-subtle hover:text-muted")} onClick={() => setTable((v) => !v)}>
          <Table2 className="size-3" /> Tabela
        </button>
      </div>
      {table ? (
        <table className="w-full text-[11.5px]">
          <tbody>
            {bars.map((b) => (
              <tr key={b.key} className="border-b border-border/50">
                <td className="py-1 text-muted">{b.label}</td>
                <td className="py-1 text-right text-fg tabular-nums">{b.value === null ? "não informado" : format(b.value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="relative" style={{ height: height + 18 }} onMouseLeave={() => setHover(null)}>
          <svg width="100%" height={height} preserveAspectRatio="none" viewBox={`0 0 100 ${height}`} className="block overflow-visible" role="img" aria-label={title}>
            {[0.5, 1].map((f) => (
              <line key={f} x1="0" x2="100" y1={height - height * f} y2={height - height * f} stroke="var(--color-border, #252c37)" strokeWidth="0.5" vectorEffect="non-scaling-stroke" opacity={0.6} />
            ))}
            {bars.map((b, i) => {
              const v = b.value ?? 0;
              const h = max > 0 ? Math.max(v > 0 ? 2 : 0, (v / max) * (height - 4)) : 0;
              return (
                <g key={b.key}>
                  {/* Hit target: the whole column, larger than the mark. */}
                  <rect x={i * w} y={0} width={w} height={height} fill="transparent" onMouseEnter={() => setHover(i)} />
                  {h > 0 && (
                    <rect
                      x={i * w + Math.min(w * 0.2, 1)}
                      y={height - h}
                      width={Math.max(0.5, w - Math.min(w * 0.4, 2))}
                      height={h}
                      rx={Math.min(1.2, w / 4)}
                      fill="#579BFF"
                      opacity={hover === null || hover === i ? 1 : 0.45}
                      pointerEvents="none"
                    />
                  )}
                  {b.value === null && <line x1={i * w + w / 2} x2={i * w + w / 2} y1={height - 3} y2={height} stroke="#5f6a7b" strokeWidth="1" vectorEffect="non-scaling-stroke" />}
                </g>
              );
            })}
            <line x1="0" x2="100" y1={height} y2={height} stroke="#5f6a7b" strokeWidth="1" vectorEffect="non-scaling-stroke" />
          </svg>
          <div className="relative h-[18px]">
            {bars.map((b, i) =>
              i % labelEvery === 0 ? (
                <span key={b.key} className="absolute top-1 -translate-x-1/2 text-[10px] whitespace-nowrap text-subtle" style={{ left: `${i * w + w / 2}%` }}>
                  {b.label}
                </span>
              ) : null,
            )}
          </div>
          {hover !== null && bars[hover] && (
            <div
              className="pointer-events-none absolute -top-1 z-10 -translate-x-1/2 -translate-y-full rounded-md border border-border bg-elevated px-2 py-1 text-[11px] whitespace-nowrap text-fg shadow-lg"
              style={{ left: `${Math.min(92, Math.max(8, hover * w + w / 2))}%` }}
            >
              {bars[hover].tooltip}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
