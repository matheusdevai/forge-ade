import { useEffect, useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { Button, Card } from "@/components/ui";
import { api, errorMessage, type Metrics } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useWorkspaces } from "@/stores/data";
import type { PaneProps } from "@/features/panes/registry";
import { Overview, ProcessTable, TimelineList } from "./parts";

type Tab = "overview" | "processes" | "timeline" | "audit";
type Range = "24h" | "7d" | "30d" | "90d";
const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "Visão geral" },
  { id: "processes", label: "Processos" },
  { id: "timeline", label: "Histórico" },
  { id: "audit", label: "Auditoria" },
];

function useMetrics(range: Range, workspaceId: string | null, refreshMs?: number) {
  const [m, setM] = useState<Metrics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  useEffect(() => {
    let alive = true;
    const load = () =>
      void api
        .obsMetrics(range, workspaceId)
        .then((x) => alive && (setM(x), setError(null)))
        .catch((e) => alive && setError(errorMessage(e)));
    load();
    const t = refreshMs ? setInterval(load, refreshMs) : null;
    return () => {
      alive = false;
      if (t) clearInterval(t);
    };
  }, [range, workspaceId, nonce, refreshMs]);
  return { m, error, reload: () => setNonce((n) => n + 1) };
}

export function ObservabilityView({ tab: initial }: { tab?: Tab }) {
  const navigate = useApp((s) => s.navigate);
  const workspaces = useWorkspaces((s) => s.items);
  const [tab, setTab] = useState<Tab>(initial ?? "overview");
  const [range, setRange] = useState<Range>("7d");
  const [ws, setWs] = useState("");
  const { m, error, reload } = useMetrics(range, ws || null);
  useEffect(() => {
    if (initial) setTab(initial);
  }, [initial]);
  useEffect(() => {
    if (!useWorkspaces.getState().loaded) void useWorkspaces.getState().refresh().catch(() => undefined);
  }, []);
  const select = "h-8 rounded-md border border-border bg-bg px-2 text-[12.5px] text-fg outline-none";

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="mx-auto flex w-full max-w-6xl min-h-0 flex-1 flex-col px-8 py-8">
        <div className="mb-4 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Observabilidade</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Métricas reais do histórico desta máquina. Tokens e custos aparecem só quando o CLI os informou — o FORGE mostra a cobertura e nunca estima.
            </p>
          </div>
          {tab === "overview" && (
            <div className="ml-auto flex items-center gap-2">
              <select className={select} value={ws} onChange={(e) => setWs(e.target.value)} aria-label="Workspace">
                <option value="">Todos os workspaces</option>
                {workspaces.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
              <select className={select} value={range} onChange={(e) => setRange(e.target.value as Range)} aria-label="Período">
                <option value="24h">24 horas</option>
                <option value="7d">7 dias</option>
                <option value="30d">30 dias</option>
                <option value="90d">90 dias</option>
              </select>
              <Button size="icon" variant="ghost" onClick={reload} aria-label="Atualizar">
                <RefreshCw />
              </Button>
            </div>
          )}
        </div>
        <div className="mb-4 flex w-fit gap-1 rounded-md border border-border bg-bg p-0.5">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => (setTab(t.id), navigate({ view: "observability", tab: t.id }))}
              className={cn("rounded px-2.5 py-1 text-[12px]", tab === t.id ? "bg-elevated text-fg" : "text-muted hover:text-fg")}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div className={cn("min-h-0 flex-1", tab === "overview" || tab === "processes" ? "overflow-auto" : "")}>
          {tab === "overview" && (error ? <p className="text-[12px] text-error">{error}</p> : m ? <Overview metrics={m} /> : <Loader2 className="size-4 animate-spin text-muted" />)}
          {tab === "processes" && (
            <Card className="p-3">
              <ProcessTable />
              <p className="mt-2 text-[10.5px] text-subtle">CPU e memória medidos pelo sistema (ps) para o processo principal de cada item, atualizados a cada 3 s.</p>
            </Card>
          )}
          {tab === "timeline" && <TimelineList />}
          {tab === "audit" && (
            <div className="flex h-full min-h-0 flex-col">
              <p className="mb-2 text-[11.5px] text-muted">
                Ações sensíveis: execuções com acesso total, chaves e segredos, push, descarte de alterações, exclusões, instalação de skills, rotinas e pausas gerais. Valores de segredos
                nunca são registrados.
              </p>
              <div className="min-h-0 flex-1">
                <TimelineList audit />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** Workspace pane: live history of this workspace. */
export function LogsPane({ workspaceId }: PaneProps) {
  return (
    <div className="h-full p-2">
      <TimelineList fixedWorkspaceId={workspaceId} compact refreshMs={5000} />
    </div>
  );
}

/** Workspace pane: processes + last 24 h metrics of this workspace. */
export function MonitorPane({ workspaceId }: PaneProps) {
  const { m, error } = useMetrics("24h", workspaceId, 10_000);
  return (
    <div className="h-full space-y-3 overflow-auto p-3">
      <ProcessTable workspaceId={workspaceId} />
      {error ? <p className="text-[12px] text-error">{error}</p> : m ? <Overview metrics={m} compact /> : <Loader2 className="size-4 animate-spin text-muted" />}
    </div>
  );
}
