import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Activity, Bot, Clapperboard, Cpu, Loader2, Pause, Play, RotateCcw, Search, Target, Terminal, Timer, Wrench } from "lucide-react";
import { Badge, Button, Card, Dialog, DialogContent, EmptyState, Input } from "@/components/ui";
import { api, errorMessage, type Metrics, type ProcessInfo, type ReplayStep, type TimelineItem, type TimelineQuery, type TimelineSource } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useWorkspaces } from "@/stores/data";
import { BarChart } from "./BarChart";
import { coverage, fmtCompact, fmtInt, fmtKb, fmtMs, fmtUsd, reportedValue } from "./format";

export const SOURCES: { id: TimelineSource; label: string; icon: typeof Bot }[] = [
  { id: "agent", label: "Agentes", icon: Bot },
  { id: "tool", label: "Comandos e ferramentas", icon: Wrench },
  { id: "mission", label: "Missões", icon: Target },
  { id: "routine", label: "Rotinas", icon: Timer },
  { id: "media", label: "Mídia", icon: Clapperboard },
  { id: "activity", label: "Atividade", icon: Activity },
];
const SOURCE_ICON = Object.fromEntries(SOURCES.map((s) => [s.id, s.icon])) as Record<TimelineSource, typeof Bot>;
const LEVEL_TONE = { info: "neutral", success: "success", warning: "warning", error: "error" } as const;
const LEVEL_LABEL = { info: "info", success: "ok", warning: "aviso", error: "erro" } as const;

// ── Replay ────────────────────────────────────────────────────────────────

const SPEEDS = [1, 4, 16];

export function ReplayDialog({ runId, onClose }: { runId: string; onClose: () => void }) {
  const [steps, setSteps] = useState<ReplayStep[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [shown, setShown] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [speed, setSpeed] = useState(4);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api
      .obsReplay(runId)
      .then(setSteps)
      .catch((e) => setError(errorMessage(e)));
  }, [runId]);

  // Schedules the next step using the original gap between events.
  useEffect(() => {
    if (!steps || !playing || shown >= steps.length) return;
    const prev = shown === 0 ? 0 : steps[shown - 1]!.offsetMs;
    const gap = Math.min(4000, Math.max(0, steps[shown]!.offsetMs - prev) / speed);
    const t = setTimeout(() => setShown((s) => s + 1), gap);
    return () => clearTimeout(t);
  }, [steps, playing, shown, speed]);

  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [shown]);

  const done = steps !== null && shown >= steps.length;
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title="Reprodução da execução" description="Eventos gravados do agente, no ritmo original (acelerável). Segredos já foram mascarados na gravação." className="w-[min(760px,calc(100vw-32px))]">
        <div className="flex items-center gap-2 border-b border-border px-5 py-2">
          <Button size="sm" variant="secondary" disabled={!steps || done} onClick={() => setPlaying((p) => !p)}>
            {playing && !done ? <Pause /> : <Play />} {playing && !done ? "Pausar" : "Continuar"}
          </Button>
          <Button size="sm" variant="ghost" disabled={!steps} onClick={() => (setShown(0), setPlaying(true))}>
            <RotateCcw /> Recomeçar
          </Button>
          <Button size="sm" variant="ghost" disabled={!steps || done} onClick={() => steps && setShown(steps.length)}>
            Mostrar tudo
          </Button>
          <div className="ml-auto flex rounded-md border border-border p-0.5">
            {SPEEDS.map((s) => (
              <button key={s} onClick={() => setSpeed(s)} className={cn("rounded px-1.5 py-0.5 text-[11px]", speed === s ? "bg-elevated text-fg" : "text-muted")}>
                {s}×
              </button>
            ))}
          </div>
          {steps && (
            <span className="text-[11px] text-subtle tabular-nums">
              {shown}/{steps.length}
            </span>
          )}
        </div>
        <div ref={listRef} className="h-[55vh] overflow-auto px-5 py-3 font-mono text-[11.5px] leading-relaxed">
          {error ? (
            <p className="text-error">{error}</p>
          ) : !steps ? (
            <Loader2 className="size-4 animate-spin text-muted" />
          ) : steps.length === 0 ? (
            <p className="text-subtle">Esta execução não tem eventos gravados.</p>
          ) : (
            steps.slice(0, shown).map((s) => (
              <div key={s.seq} className="flex gap-3 border-b border-border/40 py-1.5">
                <span className="w-14 shrink-0 text-right text-subtle tabular-nums">+{(s.offsetMs / 1000).toFixed(1)}s</span>
                <span className={cn("w-20 shrink-0", s.kind === "error" ? "text-error" : s.kind === "tool_use" ? "text-accent" : s.kind === "result" ? "text-success" : "text-muted")}>{s.kind}</span>
                <span className="min-w-0 whitespace-pre-wrap break-words text-fg">{s.text}</span>
              </div>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ── Timeline ──────────────────────────────────────────────────────────────

export function TimelineList({ fixedWorkspaceId, audit, compact, refreshMs }: { fixedWorkspaceId?: string | null; audit?: boolean; compact?: boolean; refreshMs?: number }) {
  const workspaces = useWorkspaces((s) => s.items);
  const navigate = useApp((s) => s.navigate);
  const [query, setQuery] = useState("");
  const [sources, setSources] = useState<TimelineSource[]>([]);
  const [level, setLevel] = useState("");
  const [ws, setWs] = useState(fixedWorkspaceId ?? "");
  const [days, setDays] = useState(7);
  const [items, setItems] = useState<TimelineItem[] | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [replay, setReplay] = useState<string | null>(null);

  const q: TimelineQuery = useMemo(
    () => ({
      query: query.trim() || null,
      sources: sources.length ? sources : null,
      level: level || null,
      workspaceId: (fixedWorkspaceId ?? ws) || null,
      since: Date.now() - days * 86_400_000,
      limit: compact ? 150 : 400,
      audit: !!audit,
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [query, sources, level, ws, days, fixedWorkspaceId, audit, compact],
  );
  const load = useCallback(() => {
    api
      .obsTimeline(q)
      .then(setItems)
      .catch(() => setItems([]));
  }, [q]);
  useEffect(() => {
    const t = setTimeout(load, query ? 250 : 0);
    return () => clearTimeout(t);
  }, [load, query]);
  useEffect(() => {
    if (!refreshMs) return;
    const t = setInterval(load, refreshMs);
    return () => clearInterval(t);
  }, [load, refreshMs]);

  const toggle = (s: TimelineSource) => setSources((cur) => (cur.includes(s) ? cur.filter((x) => x !== s) : [...cur, s]));
  const select = "h-7 rounded-md border border-border bg-bg px-1.5 text-[11.5px] text-fg outline-none";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-1.5 pb-2">
        <div className="relative min-w-[180px] flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-subtle" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Pesquisar pedidos, respostas, comandos, erros…" className="h-7 pl-7 text-[12px]" />
        </div>
        {!audit &&
          SOURCES.map((s) => (
            <button
              key={s.id}
              onClick={() => toggle(s.id)}
              title={s.label}
              className={cn("flex h-7 items-center gap-1 rounded-md border px-1.5 text-[11px]", sources.includes(s.id) ? "border-accent/50 bg-accent/10 text-accent" : "border-border text-muted hover:text-fg")}
            >
              <s.icon className="size-3" /> {!compact && s.label}
            </button>
          ))}
        <select className={select} value={level} onChange={(e) => setLevel(e.target.value)} aria-label="Nível">
          <option value="">Todos os níveis</option>
          <option value="error">Erros</option>
          <option value="warning">Avisos</option>
          <option value="success">Sucessos</option>
        </select>
        {fixedWorkspaceId === undefined && (
          <select className={select} value={ws} onChange={(e) => setWs(e.target.value)} aria-label="Workspace">
            <option value="">Todos os workspaces</option>
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.name}
              </option>
            ))}
          </select>
        )}
        <select className={select} value={days} onChange={(e) => setDays(Number(e.target.value))} aria-label="Período">
          <option value={1}>24 h</option>
          <option value={7}>7 dias</option>
          <option value={30}>30 dias</option>
          <option value={365}>1 ano</option>
        </select>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {!items ? (
          <Loader2 className="m-3 size-4 animate-spin text-muted" />
        ) : items.length === 0 ? (
          <EmptyState title="Nada encontrado" description={audit ? "Nenhuma ação sensível no período." : "Ajuste os filtros ou o período."} />
        ) : (
          items.map((it, i) => {
            const Icon = SOURCE_ICON[it.source];
            const expanded = open === i;
            return (
              <div key={i} className="border-b border-border/50">
                <button className="flex w-full items-start gap-2 px-1 py-1.5 text-left hover:bg-elevated/40" onClick={() => setOpen(expanded ? null : i)}>
                  <Icon className="mt-0.5 size-3.5 shrink-0 text-subtle" />
                  <span className={cn("min-w-0 flex-1 text-[12px] text-fg", !expanded && "truncate")}>{it.title}</span>
                  {it.level !== "info" && <Badge tone={LEVEL_TONE[it.level]}>{LEVEL_LABEL[it.level]}</Badge>}
                  <span className="shrink-0 text-[10.5px] text-subtle" title={new Date(it.ts).toLocaleString("pt-BR")}>
                    {relativeTime(it.ts)}
                  </span>
                </button>
                {expanded && (
                  <div className="space-y-2 pb-2 pl-6">
                    {it.detail && <pre className="max-h-56 overflow-auto rounded bg-bg p-2 font-mono text-[11px] whitespace-pre-wrap text-muted">{it.detail}</pre>}
                    <div className="flex flex-wrap gap-1 text-[11px]">
                      <span className="text-subtle">
                        {it.kind} · {new Date(it.ts).toLocaleString("pt-BR")}
                        {it.workspaceId && !fixedWorkspaceId ? ` · ${workspaces.find((w) => w.id === it.workspaceId)?.name ?? ""}` : ""}
                      </span>
                      {it.runId && (
                        <Button size="sm" variant="ghost" className="h-6" onClick={() => setReplay(it.runId)}>
                          <Play /> Reproduzir execução
                        </Button>
                      )}
                      {it.missionId && (
                        <Button size="sm" variant="ghost" className="h-6" onClick={() => navigate({ view: "mission", id: it.missionId! })}>
                          Abrir missão
                        </Button>
                      )}
                      {it.routineId && (
                        <Button size="sm" variant="ghost" className="h-6" onClick={() => navigate({ view: "autopilot" })}>
                          Ver rotina
                        </Button>
                      )}
                    </div>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
      {!compact && <p className="shrink-0 pt-2 text-[10.5px] text-subtle">Terminais não são gravados (privacidade): o histórico cobre agentes, ferramentas, missões, rotinas, mídia e atividade.</p>}
      {replay && <ReplayDialog runId={replay} onClose={() => setReplay(null)} />}
    </div>
  );
}

// ── Processes ─────────────────────────────────────────────────────────────

const KIND_LABEL: Record<ProcessInfo["kind"], string> = { app: "App", terminal: "Terminal", agent: "Agente", browser: "Navegador", voice: "Voz", routine: "Rotina" };

export function ProcessTable({ workspaceId, refreshMs = 3000 }: { workspaceId?: string | null; refreshMs?: number }) {
  const [items, setItems] = useState<ProcessInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    const load = () =>
      void api
        .obsProcesses()
        .then((p) => alive && (setItems(p), setError(null)))
        .catch((e) => alive && setError(errorMessage(e)));
    load();
    const t = setInterval(load, refreshMs);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [refreshMs]);
  const shown = (items ?? []).filter((p) => !workspaceId || p.workspaceId === workspaceId || (p.kind !== "terminal" && p.kind !== "agent" && p.kind !== "routine" && p.kind !== "app"));
  if (error) return <p className="text-[12px] text-error">{error}</p>;
  if (!items) return <Loader2 className="size-4 animate-spin text-muted" />;
  return (
    <table className="w-full text-[12px]">
      <thead>
        <tr className="border-b border-border text-left text-[11px] text-subtle">
          <th className="py-1.5 font-normal">Tipo</th>
          <th className="py-1.5 font-normal">Processo</th>
          <th className="py-1.5 text-right font-normal">PID</th>
          <th className="py-1.5 text-right font-normal">CPU</th>
          <th className="py-1.5 text-right font-normal">Memória</th>
          <th className="py-1.5 text-right font-normal">Desde</th>
        </tr>
      </thead>
      <tbody>
        {shown.map((p, i) => (
          <tr key={i} className="border-b border-border/50">
            <td className="py-1.5">
              <Badge tone={p.kind === "voice" ? "warning" : p.kind === "agent" || p.kind === "routine" ? "accent" : "neutral"}>
                {p.kind === "terminal" ? <Terminal className="size-3" /> : p.kind === "app" ? <Cpu className="size-3" /> : null}
                {KIND_LABEL[p.kind]}
              </Badge>
            </td>
            <td className="max-w-[340px] truncate py-1.5 text-fg" title={p.label}>
              {p.label}
            </td>
            <td className="py-1.5 text-right text-subtle tabular-nums">{p.pid ?? "—"}</td>
            <td className="py-1.5 text-right tabular-nums">{p.cpuPercent === null ? "—" : `${p.cpuPercent.toFixed(1)}%`}</td>
            <td className="py-1.5 text-right tabular-nums">{fmtKb(p.rssKb)}</td>
            <td className="py-1.5 text-right text-subtle">{p.startedAt ? relativeTime(p.startedAt) : "—"}</td>
          </tr>
        ))}
        {shown.length <= 1 && (
          <tr>
            <td colSpan={6} className="py-3 text-[11.5px] text-subtle">
              Nenhum terminal, agente, navegador ou rotina em execução agora.
            </td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

// ── Overview ──────────────────────────────────────────────────────────────

function Stat({ label, value, sub, tone }: { label: string; value: string; sub?: string; tone?: "error" | "success" }) {
  return (
    <Card className="px-3 py-2.5">
      <p className="text-[11px] text-muted">{label}</p>
      <p className={cn("mt-0.5 text-[20px] font-semibold tabular-nums", tone === "error" ? "text-error" : tone === "success" ? "text-success" : "text-fg")}>{value}</p>
      {sub && <p className="mt-0.5 text-[10.5px] text-subtle">{sub}</p>}
    </Card>
  );
}

const dayLabel = (ms: number, n: number) => new Date(ms).toLocaleDateString("pt-BR", n > 2 ? { day: "2-digit", month: "2-digit" } : { weekday: "short" });

export function Overview({ metrics, compact }: { metrics: Metrics; compact?: boolean }) {
  const m = metrics;
  const sum = (o: Record<string, number>) => Object.values(o).reduce((a, b) => a + b, 0);
  const n = m.days.length;
  return (
    <div className="space-y-3">
      <div className={cn("grid gap-2", compact ? "grid-cols-2" : "grid-cols-2 lg:grid-cols-4")}>
        <Stat label="Execuções de agentes" value={fmtInt(m.runs)} sub={`${fmtInt(m.completed)} ok · ${fmtInt(m.failed)} falhas · ${fmtInt(m.running)} em andamento`} tone={m.failed > 0 && m.failed >= m.completed ? "error" : undefined} />
        <Stat label="Duração (p50 / p95)" value={fmtMs(m.p50DurationMs)} sub={`p95 ${fmtMs(m.p95DurationMs)}`} />
        <Stat
          label="Tokens (entrada / saída)"
          value={`${reportedValue(m.inputTokens, fmtCompact)} / ${reportedValue(m.outputTokens, fmtCompact)}`}
          sub={coverage(m.inputTokens)}
        />
        <Stat label="Custo informado" value={reportedValue(m.costUsd, fmtUsd)} sub={`${coverage(m.costUsd)} · nada é estimado`} />
        {!compact && (
          <>
            <Stat label="Missões no período" value={fmtInt(sum(m.missions))} sub={`${m.missions.completed ?? 0} concluídas · ${m.missions.failed ?? 0} falharam · ${m.missions.running ?? 0} rodando`} />
            <Stat label="Tarefas" value={fmtInt(sum(m.tasks))} sub={`${m.tasks.approved ?? 0} aprovadas · ${(m.tasks.failed ?? 0) + (m.tasks.rejected ?? 0)} com problema`} />
            <Stat
              label="Rotinas"
              value={fmtInt(sum(m.routineRuns))}
              sub={`${m.routineRuns.failed ?? 0} falhas · ${m.routineRuns.skipped ?? 0} puladas · ${m.routinesDisabled} desativada(s)`}
              tone={(m.routineRuns.failed ?? 0) > 0 ? "error" : undefined}
            />
            <Stat label="Comandos e ferramentas" value={fmtInt(m.toolUses)} sub={`mídia: ${sum(m.media)} item(ns), ${m.media.failed ?? 0} falha(s)`} />
          </>
        )}
      </div>
      <div className={cn("grid gap-3", compact ? "" : "lg:grid-cols-2")}>
        <Card className="p-3">
          <BarChart
            title="Execuções de agentes por dia"
            format={fmtInt}
            bars={m.days.map((d) => ({ key: String(d.day), label: dayLabel(d.day, n), value: d.runs, tooltip: `${new Date(d.day).toLocaleDateString("pt-BR")}: ${d.runs} execução(ões), ${d.failed} falha(s)` }))}
          />
        </Card>
        {!compact && (
          <Card className="p-3">
            <BarChart
              title="Custo informado por dia (US$)"
              format={fmtUsd}
              bars={m.days.map((d) => ({
                key: String(d.day),
                label: dayLabel(d.day, n),
                value: d.costRuns > 0 ? d.costUsd : d.runs > 0 ? null : 0,
                tooltip:
                  d.costRuns > 0
                    ? `${new Date(d.day).toLocaleDateString("pt-BR")}: ${fmtUsd(d.costUsd ?? 0)} em ${d.costRuns} de ${d.runs} execução(ões)`
                    : d.runs > 0
                      ? `${new Date(d.day).toLocaleDateString("pt-BR")}: custo não informado pelos CLIs`
                      : `${new Date(d.day).toLocaleDateString("pt-BR")}: sem execuções`,
              }))}
            />
          </Card>
        )}
      </div>
      {!compact && m.providers.length > 0 && (
        <Card className="p-3">
          <h3 className="mb-2 text-[12.5px] font-medium">Por provider e modelo</h3>
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-border text-left text-[11px] text-subtle">
                <th className="py-1.5 font-normal">Provider</th>
                <th className="py-1.5 text-right font-normal">Execuções</th>
                <th className="py-1.5 text-right font-normal">Sucesso</th>
                <th className="py-1.5 text-right font-normal">Duração média</th>
                <th className="py-1.5 text-right font-normal">Tokens (ent./saída)</th>
                <th className="py-1.5 text-right font-normal">Custo informado</th>
              </tr>
            </thead>
            <tbody>
              {m.providers.map((p, i) => (
                <tr key={i} className="border-b border-border/50">
                  <td className="py-1.5">
                    {p.providerId}
                    {p.model && <span className="text-subtle"> · {p.model}</span>}
                  </td>
                  <td className="py-1.5 text-right tabular-nums">{fmtInt(p.runs)}</td>
                  <td className="py-1.5 text-right tabular-nums">{p.runs ? `${Math.round((p.completed / p.runs) * 100)}%` : "—"}</td>
                  <td className="py-1.5 text-right tabular-nums">{fmtMs(p.avgDurationMs)}</td>
                  <td className="py-1.5 text-right tabular-nums" title={coverage(p.inputTokens)}>
                    {reportedValue(p.inputTokens, fmtCompact)} / {reportedValue(p.outputTokens, fmtCompact)}
                  </td>
                  <td className="py-1.5 text-right tabular-nums" title={coverage(p.costUsd)}>
                    {reportedValue(p.costUsd, fmtUsd)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="mt-2 text-[10.5px] text-subtle">“—” = o CLI não informou esse dado nessas execuções. O FORGE não estima tokens nem custos.</p>
        </Card>
      )}
    </div>
  );
}
