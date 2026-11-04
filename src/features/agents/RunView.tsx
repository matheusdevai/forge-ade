import { useEffect, useMemo, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { ChevronRight, CircleStop, FileCode2, FolderGit2, GitCommitHorizontal, Loader2, TriangleAlert, Wrench } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, StatusDot } from "@/components/ui";
import { openFileInEditor } from "@/features/editor/actions";
import { openWorkspace } from "@/features/workspace/actions";
import { api, errorMessage, type AgentEvent, type AgentRun, type RunChanges, type RunMessage } from "@/lib/ipc";
import { cn, prettyPath } from "@/lib/utils";
import { useWorkspaces } from "@/stores/data";
import { PERMISSION_INFO } from "@/stores/agents";

type Seq = { seq: number; event: AgentEvent };

/** Plain text with fenced code blocks rendered as code. */
export function RichText({ text }: { text: string }) {
  const parts = text.split(/```([\w-]*)\n?([\s\S]*?)```/g);
  const out: React.ReactNode[] = [];
  for (let i = 0; i < parts.length; i += 3) {
    if (parts[i]) out.push(<span key={i} className="whitespace-pre-wrap">{parts[i]}</span>);
    if (i + 2 < parts.length) {
      out.push(
        <pre key={`c${i}`} className="selectable my-2 overflow-x-auto rounded-md border border-border bg-bg p-2.5 font-mono text-[11.5px] leading-relaxed text-fg/90">
          {parts[i + 2]}
        </pre>,
      );
    }
  }
  return <div className="selectable text-[13px] leading-relaxed text-fg">{out}</div>;
}

function ToolCall({ use, result }: { use: Extract<AgentEvent, { kind: "tool_use" }>; result?: Extract<AgentEvent, { kind: "tool_result" }> }) {
  const [open, setOpen] = useState(false);
  let summary = use.input;
  try {
    const v = JSON.parse(use.input) as Record<string, unknown>;
    summary = String(v.command ?? v.file_path ?? v.path ?? v.pattern ?? v.query ?? v.url ?? use.input);
  } catch {
    /* plain string input */
  }
  return (
    <div className="my-1 rounded-md border border-border bg-bg-2">
      <button type="button" onClick={() => setOpen((o) => !o)} className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left">
        <ChevronRight className={cn("size-3 shrink-0 text-subtle transition-transform", open && "rotate-90")} />
        <Wrench className="size-3 shrink-0 text-accent-2" />
        <span className="shrink-0 text-[12px] font-medium text-fg">{use.name}</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted">{summary}</span>
        {!result ? (
          <Loader2 className="size-3 shrink-0 animate-spin text-subtle" />
        ) : (
          <StatusDot tone={result.ok ? "success" : "error"} />
        )}
      </button>
      {open && (
        <div className="space-y-1.5 border-t border-border p-2.5">
          <pre className="selectable max-h-48 overflow-auto whitespace-pre-wrap font-mono text-[11px] text-muted">{use.input}</pre>
          {result && (
            <pre className={cn("selectable max-h-72 overflow-auto whitespace-pre-wrap rounded bg-bg p-2 font-mono text-[11px]", result.ok ? "text-fg/80" : "text-error")}>
              {result.output || (result.ok ? "(sem saída)" : "falhou")}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

const STATUS_TONE = { running: "accent", completed: "success", failed: "error", cancelled: "neutral" } as const;
const STATUS_LABEL = { running: "em execução", completed: "concluída", failed: "falhou", cancelled: "cancelada" } as const;

function Changes({ run }: { run: AgentRun }) {
  const [data, setData] = useState<RunChanges | null>(null);
  const [loading, setLoading] = useState(false);
  const workspaces = useWorkspaces((s) => s.items);
  const load = () => {
    setLoading(true);
    api
      .agentRunChanges(run.id)
      .then(setData)
      .catch((e) => toast.error("Falha ao ler alterações", { description: errorMessage(e) }))
      .finally(() => setLoading(false));
  };
  if (!data)
    return (
      <Button size="sm" variant="ghost" className="h-6 px-2" disabled={loading} onClick={load}>
        {loading ? <Loader2 className="animate-spin" /> : <GitCommitHorizontal />} Alterações
      </Button>
    );
  const openWt = async () => {
    try {
      const existing = workspaces.find((w) => w.path === data.cwd);
      const ws = existing ?? (await useWorkspaces.getState().add(data.cwd));
      await openWorkspace(ws.id);
    } catch (e) {
      toast.error("Não foi possível abrir a worktree", { description: errorMessage(e) });
    }
  };
  return (
    <div className="mt-2 w-full rounded-md border border-border bg-bg p-2 text-[12px]">
      <div className="mb-1 flex items-center gap-2 text-subtle">
        <span>
          {data.files.length} arquivo(s) alterado(s) · {data.commits.length} commit(s) desde o início
        </span>
        {data.worktreeBranch && (
          <Button size="sm" variant="ghost" className="ml-auto h-6 px-2" onClick={() => void openWt()}>
            <FolderGit2 /> Abrir worktree
          </Button>
        )}
      </div>
      {!data.isRepo && <p className="text-subtle">O diretório não é um repositório Git — alterações não podem ser listadas.</p>}
      {data.commits.map((c) => (
        <div key={c.sha} className="truncate font-mono text-[11px] text-muted">
          {c.short} {c.subject}
        </div>
      ))}
      {data.files.map((f) => {
        const rel = data.prefix ? (f.path.startsWith(data.prefix) ? f.path.slice(data.prefix.length) : null) : f.path;
        return (
          <button
            key={f.path}
            type="button"
            disabled={!rel || !!data.worktreeBranch}
            onClick={() => rel && openFileInEditor(rel)}
            className="flex w-full items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-elevated disabled:hover:bg-transparent"
            title={data.worktreeBranch ? "Abra a worktree para editar" : f.path}
          >
            <span className="w-3 font-mono text-[11px] text-warning">{f.untracked ? "U" : f.worktree !== "." ? f.worktree : f.index}</span>
            <FileCode2 className="size-3 text-subtle" />
            <span className="truncate text-muted">{f.path}</span>
          </button>
        );
      })}
    </div>
  );
}

export function RunView({ runId, onUpdate }: { runId: string; onUpdate: (run: AgentRun) => void }) {
  const [run, setRun] = useState<AgentRun | null>(null);
  const [events, setEvents] = useState<Seq[]>([]);
  const [delta, setDelta] = useState("");
  const [error, setError] = useState<string | null>(null);
  const onUpdateRef = useRef(onUpdate);
  onUpdateRef.current = onUpdate;

  useEffect(() => {
    let disposed = false;
    let token: number | null = null;
    const channel = new Channel<RunMessage>();
    channel.onmessage = (m) => {
      if (disposed) return;
      if (m.type === "status") {
        setRun(m.run);
        setDelta("");
        onUpdateRef.current(m.run);
        return;
      }
      if (m.event.kind === "text_delta") {
        const t = m.event.text;
        setDelta((d) => d + t);
        return;
      }
      if (m.event.kind === "text" || m.event.kind === "tool_use") setDelta("");
      const item = { seq: m.seq, event: m.event };
      setEvents((es) => (es.some((e) => e.seq === m.seq) ? es : [...es, item]));
    };
    api
      .agentRunAttach(runId, channel)
      .then((res) => {
        if (disposed) {
          if (res.token !== null) void api.agentRunDetach(runId, res.token);
          return;
        }
        token = res.token;
        setRun(res.run);
        onUpdateRef.current(res.run);
        setEvents((live) => {
          const map = new Map<number, Seq>();
          for (const e of res.events) map.set(e.seq, { seq: e.seq, event: e.event });
          for (const e of live) map.set(e.seq, e);
          return [...map.values()].sort((a, b) => a.seq - b.seq);
        });
      })
      .catch((e) => !disposed && setError(errorMessage(e)));
    return () => {
      disposed = true;
      if (token !== null) void api.agentRunDetach(runId, token).catch(() => undefined);
    };
  }, [runId]);

  const items = useMemo(() => {
    const results = new Map<string, Extract<AgentEvent, { kind: "tool_result" }>>();
    for (const { event } of events) if (event.kind === "tool_result" && event.id) results.set(event.id, event);
    return events.filter(({ event }) => !(event.kind === "tool_result" && event.id) && event.kind !== "result" && event.kind !== "session").map((e) => ({
      ...e,
      result: e.event.kind === "tool_use" && e.event.id ? results.get(e.event.id) : undefined,
    }));
  }, [events]);

  if (error) return <p className="px-4 py-2 text-[12px] text-error">{error}</p>;
  if (!run)
    return (
      <div className="flex justify-center py-4 text-subtle">
        <Loader2 className="size-4 animate-spin" />
      </div>
    );

  return (
    <div className="border-b border-border px-4 py-3">
      <div className="mb-2 flex justify-end">
        <div className="selectable max-w-[85%] rounded-lg rounded-br-sm bg-accent/15 px-3 py-2 text-[13px] whitespace-pre-wrap text-fg">{run.prompt}</div>
      </div>
      <div className="space-y-1.5">
        {items.map(({ seq, event, result }) => {
          switch (event.kind) {
            case "text":
              return <RichText key={seq} text={event.text} />;
            case "tool_use":
              return <ToolCall key={seq} use={event} result={result} />;
            case "tool_result":
              return (
                <pre key={seq} className="max-h-40 overflow-auto rounded bg-bg-2 p-2 font-mono text-[11px] text-muted">
                  {event.output}
                </pre>
              );
            case "warning":
              return (
                <p key={seq} className="flex items-start gap-1.5 text-[11.5px] text-warning/80">
                  <TriangleAlert className="mt-0.5 size-3 shrink-0" /> {event.message}
                </p>
              );
            case "error":
              return (
                <p key={seq} className="flex items-start gap-1.5 text-[12px] text-error">
                  <TriangleAlert className="mt-0.5 size-3.5 shrink-0" /> {event.message}
                </p>
              );
            default:
              return null;
          }
        })}
        {delta && <RichText text={delta} />}
        {run.status === "running" && !delta && (
          <div className="flex items-center gap-2 text-[12px] text-subtle">
            <Loader2 className="size-3.5 animate-spin" /> trabalhando…
          </div>
        )}
        {run.status === "failed" && run.error && !events.some((e) => e.event.kind === "error") && (
          <p className="flex items-start gap-1.5 text-[12px] text-error">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" /> {run.error}
          </p>
        )}
      </div>
      <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10.5px] text-subtle">
        <Badge tone={STATUS_TONE[run.status]}>{STATUS_LABEL[run.status]}</Badge>
        <span>{PERMISSION_INFO[run.permission].short}</span>
        {run.model && <span className="font-mono">{run.model}</span>}
        {run.durationMs !== null && <span>{(run.durationMs / 1000).toFixed(1)} s</span>}
        {(run.inputTokens !== null || run.outputTokens !== null) && (
          <span>
            tokens {run.inputTokens ?? "—"} → {run.outputTokens ?? "—"}
            {run.cacheReadTokens ? ` (+${run.cacheReadTokens} cache)` : ""}
          </span>
        )}
        {run.costUsd !== null && <span title="Valor informado pelo próprio CLI (no Claude Code com assinatura, é o equivalente de API).">US$ {run.costUsd.toFixed(4)} informado</span>}
        {run.mcp.length > 0 && <span title={run.mcp.join(", ")}>mcp: {run.mcp.join(", ")}</span>}
        {(run.memory?.length ?? 0) > 0 && <span title={run.memory.join("\n")}>memória: {run.memory.length}</span>}
        {run.skills.length > 0 && <span title={run.skills.join(", ")}>skills: {run.skills.map((s) => s.split("@")[0]).join(", ")}</span>}
        {run.worktreeBranch && (
          <span className="font-mono" title={prettyPath(run.cwd)}>
            ⎇ {run.worktreeBranch}
          </span>
        )}
        <span className="ml-auto flex items-center gap-1">
          {run.status === "running" ? (
            <Button
              size="sm"
              variant="ghost"
              className="h-6 px-2"
              onClick={() => void api.agentRunCancel(run.id).catch((e) => toast.error("Falha ao cancelar", { description: errorMessage(e) }))}
            >
              <CircleStop /> Cancelar
            </Button>
          ) : (
            <Changes run={run} />
          )}
        </span>
      </div>
    </div>
  );
}
