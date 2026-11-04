import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowLeft,
  Ban,
  BookOpen,
  Check,
  ChevronRight,
  ClipboardCopy,
  FileText,
  ShieldCheck,
  FolderGit2,
  GitMerge,
  Loader2,
  Pause,
  Pencil,
  Play,
  RotateCcw,
  SkipForward,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, CardHeader, Dialog, DialogContent, EmptyState, Input, Label, StatusDot, Switch, Textarea } from "@/components/ui";
import { RunView } from "@/features/agents/RunView";
import { openWorkspace } from "@/features/workspace/actions";
import { api, errorMessage, type MaestroSettings, type MissionDetail, type MissionMode, type MissionTask, type TaskAction, type TaskStatus } from "@/lib/ipc";
import { useProviders } from "@/stores/providers";
import { Markdown } from "./Markdown";
import { useLibrary } from "@/stores/library";
import { SkillChips } from "@/features/skills/SkillChips";
import { McpChips } from "@/features/mcp/McpChips";
import { cn, relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useAgents } from "@/stores/agents";
import { useMissions, useWorkspaces } from "@/stores/data";
import { MissionStatusBadge, MODE_INFO } from "./MissionStatusBadge";
import { PlanEditor, toInputs } from "./PlanEditor";

const TASK_STATUS: Record<TaskStatus, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "error" }> = {
  pending: { label: "Aguardando", tone: "neutral" },
  running: { label: "Em execução", tone: "accent" },
  review: { label: "Em revisão", tone: "warning" },
  approved: { label: "Aprovada", tone: "success" },
  rejected: { label: "Rejeitada", tone: "error" },
  failed: { label: "Falhou", tone: "error" },
  cancelled: { label: "Cancelada", tone: "neutral" },
  skipped: { label: "Pulada", tone: "neutral" },
};

const fmtTokens = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

function TaskCard({ detail, task, act }: { detail: MissionDetail; task: MissionTask; act: (t: MissionTask, a: TaskAction, needsFeedback?: boolean) => void }) {
  const profiles = useAgents((s) => s.profiles);
  const workspaces = useWorkspaces((s) => s.items);
  const [open, setOpen] = useState(task.status === "running" || task.status === "review");
  useEffect(() => {
    if (task.status === "running" || task.status === "review") setOpen(true);
  }, [task.status]);
  const s = TASK_STATUS[task.status];
  const providers = useProviders((st) => st.items);
  const providerName = (id: string) => providers.find((p) => p.id === id)?.name ?? id;
  const [showReview, setShowReview] = useState(false);
  const reviewing = task.status === "review" && !!task.reviewRunId && !task.reviewVerdict && !task.reviewSummary;
  const deps = detail.tasks.filter((t) => task.dependsOn.includes(t.id));
  const profile = profiles.find((p) => p.id === task.profileId);

  const openWorktree = async () => {
    if (!task.worktreePath) return;
    try {
      const existing = workspaces.find((w) => w.path === task.worktreePath);
      const ws = existing ?? (await useWorkspaces.getState().add(task.worktreePath));
      await openWorkspace(ws.id);
    } catch (e) {
      toast.error("Não foi possível abrir a worktree", { description: errorMessage(e) });
    }
  };

  return (
    <Card className={cn(task.status === "review" && "border-warning/40", task.status === "running" && "border-accent/40")}>
      <div className="flex items-start gap-3 p-3">
        <button type="button" onClick={() => setOpen((o) => !o)} className="mt-0.5" aria-label={open ? "Recolher" : "Expandir"}>
          <ChevronRight className={cn("size-4 text-subtle transition-transform", open && "rotate-90")} />
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-[11px] text-subtle">{task.key}</span>
            <span className="text-[13.5px] font-medium text-fg">{task.title}</span>
            <Badge tone={s.tone}>
              {task.status === "running" && <Loader2 className="size-3 animate-spin" />} {s.label}
            </Badge>
            {task.attempts > 1 && <Badge>tentativa {task.attempts}</Badge>}
            {task.providerOverride && <Badge tone="accent">reatribuída: {providerName(task.providerOverride)}</Badge>}
            {task.integratedAt && <Badge tone="success">integrada</Badge>}
          </div>
          <div className="mt-0.5 flex flex-wrap gap-x-3 text-[11.5px] text-subtle">
            <span>{profile?.name ?? "agente removido"}</span>
            {deps.length > 0 && <span>depois de: {deps.map((d) => d.key).join(", ")}</span>}
            {task.worktreeBranch && <span className="font-mono">⎇ {task.worktreeBranch}</span>}
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap justify-end gap-1">
          {task.status === "review" && (
            <>
              <Button size="sm" variant="ghost" onClick={() => act(task, "reject", true)}>
                <X /> Rejeitar
              </Button>
              <Button size="sm" variant="primary" onClick={() => act(task, "approve")}>
                <Check /> Aprovar
              </Button>
            </>
          )}
          {(task.status === "failed" || task.status === "rejected" || task.status === "cancelled") && (
            <Button size="sm" onClick={() => act(task, "retry", true)}>
              <RotateCcw /> Refazer
            </Button>
          )}
          {(task.status === "pending" || task.status === "failed" || task.status === "rejected" || task.status === "cancelled") && detail.mission.status !== "draft" && detail.mission.status !== "planned" && (
            <Button size="sm" variant="ghost" onClick={() => act(task, "skip")}>
              <SkipForward /> Pular
            </Button>
          )}
          {task.status === "running" && (
            <Button size="sm" variant="ghost" onClick={() => act(task, "cancel")}>
              <Ban /> Cancelar
            </Button>
          )}
          {task.worktreePath && (
            <Button size="sm" variant="ghost" onClick={() => void openWorktree()} title="Abrir a worktree desta tarefa como workspace">
              <FolderGit2 />
            </Button>
          )}
        </div>
      </div>
      {(reviewing || task.reviewVerdict || task.reviewSummary) && (
        <div
          className={cn(
            "mx-3 mb-3 rounded-md border px-3 py-2 text-[12px]",
            task.reviewVerdict === "approve" ? "border-success/30 bg-success/5" : task.reviewVerdict === "reject" ? "border-error/30 bg-error/5" : "border-border bg-bg-2",
          )}
        >
          <div className="flex items-center gap-2">
            <ShieldCheck className={cn("size-3.5", task.reviewVerdict === "approve" ? "text-success" : task.reviewVerdict === "reject" ? "text-error" : "text-subtle")} />
            <span className="font-medium text-fg">
              {reviewing ? "Maestro revisando…" : task.reviewVerdict === "approve" ? "Maestro recomenda aprovar" : task.reviewVerdict === "reject" ? "Maestro recomenda rejeitar" : "Revisão do Maestro"}
            </span>
            {reviewing && <Loader2 className="size-3 animate-spin text-subtle" />}
            {task.reviewRunId && (
              <button type="button" onClick={() => setShowReview((v) => !v)} className="ml-auto text-[11px] text-subtle hover:text-muted">
                {showReview ? "ocultar revisão" : "ver revisão"}
              </button>
            )}
          </div>
          {task.reviewSummary && <p className="selectable mt-1 whitespace-pre-wrap text-muted">{task.reviewSummary}</p>}
          {showReview && task.reviewRunId && (
            <div className="mt-2 rounded border border-border bg-bg">
              <RunView key={task.reviewRunId} runId={task.reviewRunId} onUpdate={() => undefined} />
            </div>
          )}
        </div>
      )}
      {open && (
        <div className="border-t border-border">
          <div className="grid gap-3 px-4 py-3 text-[12.5px] md:grid-cols-2">
            <div>
              <div className="mb-1 text-[10.5px] uppercase tracking-wider text-subtle">Instruções</div>
              <p className="selectable whitespace-pre-wrap text-muted">{task.description}</p>
            </div>
            <div>
              <div className="mb-1 text-[10.5px] uppercase tracking-wider text-subtle">Critérios de aceite</div>
              <p className="selectable whitespace-pre-wrap text-muted">{task.acceptance || "—"}</p>
              {task.feedback && (
                <>
                  <div className="mt-2 mb-1 text-[10.5px] uppercase tracking-wider text-warning">Feedback da revisão</div>
                  <p className="selectable whitespace-pre-wrap text-warning/90">{task.feedback}</p>
                </>
              )}
            </div>
          </div>
          {task.runId && (
            <div className="border-t border-border bg-bg-2/50">
              <RunView key={task.runId} runId={task.runId} onUpdate={() => undefined} />
            </div>
          )}
        </div>
      )}
    </Card>
  );
}

export function MissionView({ id }: { id: string }) {
  const navigate = useApp((s) => s.navigate);
  const workspaces = useWorkspaces((s) => s.items);
  const agentsLoaded = useAgents((s) => s.loaded);
  const squads = useLibrary((s) => s.squads);
  const libraryLoaded = useLibrary((s) => s.loaded);
  const [saveRecipe, setSaveRecipe] = useState<string | null>(null);
  const [detail, setDetail] = useState<MissionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [editingPlan, setEditingPlan] = useState(false);
  const [editingBrief, setEditingBrief] = useState(false);
  const [feedback, setFeedback] = useState<{ task: MissionTask; action: TaskAction; text: string } | null>(null);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [showPlanner, setShowPlanner] = useState(false);

  const load = useCallback(() => {
    api
      .missionDetail(id)
      .then((d) => {
        setDetail(d);
        setError(null);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [id]);

  useEffect(() => {
    load();
    if (!agentsLoaded) void useAgents.getState().refresh().catch(() => undefined);
    if (!libraryLoaded) void useLibrary.getState().refresh().catch(() => undefined);
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void listen<string>("forge://mission-changed", (e) => {
      if (e.payload === id) load();
    }).then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [id, load, agentsLoaded, libraryLoaded]);

  const run = async (label: string, fn: () => Promise<unknown>, ok?: string) => {
    setBusy(label);
    try {
      await fn();
      if (ok) toast.success(ok);
    } catch (e) {
      toast.error(`${label} falhou`, { description: errorMessage(e), duration: 9000 });
    } finally {
      setBusy(null);
      load();
      void useMissions.getState().refresh();
    }
  };

  const act = (t: MissionTask, action: TaskAction, needsFeedback = false) => {
    if (needsFeedback) {
      const maestro = t.reviewVerdict === "reject" ? (t.reviewSummary ?? "") : "";
      setFeedback({ task: t, action, text: action === "retry" ? (t.feedback ?? maestro) : maestro });
      return;
    }
    void run("Ação na tarefa", () => api.missionTaskAction(id, t.id, action));
  };

  if (error) return <EmptyState className="h-full" title="Missão indisponível" description={error} />;
  if (!detail)
    return (
      <div className="flex h-full items-center justify-center text-subtle">
        <Loader2 className="size-5 animate-spin" />
      </div>
    );

  const { mission: m, tasks, events, totals } = detail;
  const ws = workspaces.find((w) => w.id === m.workspaceId);
  const editable = (m.status === "draft" || m.status === "planned") && !m.planning;
  const squad = squads.find((s) => s.id === m.squadId);
  const counts = tasks.reduce<Record<string, number>>((acc, t) => ((acc[t.status] = (acc[t.status] ?? 0) + 1), acc), {});
  const done = (counts.approved ?? 0) + (counts.skipped ?? 0);
  const needsAttention = (counts.review ?? 0) + (counts.failed ?? 0) + (counts.rejected ?? 0);

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-6">
        <button type="button" onClick={() => navigate({ view: "missions" })} className="mb-3 flex items-center gap-1 text-xs text-subtle hover:text-muted">
          <ArrowLeft className="size-3.5" /> Missões
        </button>

        <div className="flex flex-wrap items-start gap-3">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="text-xl font-semibold">{m.title}</h1>
              <MissionStatusBadge status={m.status} />
              <Badge>{MODE_INFO[m.mode].label}</Badge>
              {m.integratedAt && <Badge tone="success">integrada</Badge>}
              {squad && <Badge tone="accent">{squad.name}</Badge>}
            </div>
            <p className="mt-1 text-xs text-subtle">
              {ws ? ws.name : "sem workspace"} {m.baseBranch && `· base ${m.baseBranch}`} {m.baseCommit && `@ ${m.baseCommit.slice(0, 7)}`} · criada {relativeTime(m.createdAt)}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            {editable && (
              <select
                aria-label="Squad"
                value={m.squadId ?? ""}
                onChange={(e) => void run("Squad", () => api.missionSetSquad(id, e.target.value || null))}
                className="h-8 rounded-md border border-border bg-bg-2 px-2 text-[12.5px] text-fg"
                title="O planejador distribui tarefas apenas entre os membros do squad"
              >
                <option value="">Sem squad</option>
                {squads.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            )}
            {tasks.length > 0 && (
              <Button variant="ghost" onClick={() => setSaveRecipe(`${m.title}`)} title="Reutilizar este briefing, squad, modo e plano">
                <BookOpen /> Salvar como receita
              </Button>
            )}
            {editable && (
              <>
                <Button variant="ghost" onClick={() => setEditingBrief(true)}>
                  <Pencil /> Briefing
                </Button>
                {tasks.length > 0 && (
                  <Button variant="ghost" onClick={() => setEditingPlan(true)}>
                    <Pencil /> Editar plano
                  </Button>
                )}
                <Button variant={tasks.length ? "ghost" : "primary"} disabled={!!busy || !m.workspaceId} onClick={() => void run("Planejamento", () => api.missionPlan(id))}>
                  <Sparkles /> {tasks.length ? "Replanejar" : "Gerar plano com IA"}
                </Button>
                {tasks.length === 0 && (
                  <Button variant="ghost" onClick={() => setEditingPlan(true)}>
                    Plano manual
                  </Button>
                )}
                {m.status === "planned" && tasks.length > 0 && (
                  <Button variant="primary" disabled={!!busy} onClick={() => void run("Iniciar missão", () => api.missionStart(id), "Missão iniciada")}>
                    <Play /> Aprovar plano e iniciar
                  </Button>
                )}
              </>
            )}
            {m.planning && (
              <Button variant="ghost" onClick={() => setShowPlanner((v) => !v)}>
                <Loader2 className="animate-spin" /> Planejando… {showPlanner ? "ocultar" : "ver"}
              </Button>
            )}
            {m.status === "running" && (
              <Button variant="ghost" disabled={!!busy} onClick={() => void run("Pausar", () => api.missionPause(id))}>
                <Pause /> Pausar
              </Button>
            )}
            {m.status === "paused" && (
              <Button variant="primary" disabled={!!busy} onClick={() => void run("Retomar", () => api.missionResume(id))}>
                <Play /> Retomar
              </Button>
            )}
            {(m.status === "running" || m.status === "paused" || m.status === "planned") && (
              <Button variant="danger" disabled={!!busy} onClick={() => setConfirmCancel(true)}>
                <Ban /> Cancelar missão
              </Button>
            )}
            {m.status === "completed" && (
              <>
                {!m.integratedAt && (
                  <Button variant="primary" disabled={!!busy} onClick={() => void run("Integração", () => api.missionIntegrate(id), "Alterações integradas")}>
                    <GitMerge /> Integrar em {m.baseBranch ?? "HEAD"}
                  </Button>
                )}
                <Button variant="ghost" disabled={!!busy} onClick={() => void run("Limpeza", () => api.missionCleanup(id), "Worktrees removidas (branches mantidas)")}>
                  <Trash2 /> Limpar worktrees
                </Button>
              </>
            )}
          </div>
        </div>

        {(m.planning || showPlanner) && m.planRunId && (
          <Card className="mt-4 overflow-hidden">
            <CardHeader title="Planejamento" icon={<Sparkles />} />
            <RunView key={m.planRunId} runId={m.planRunId} onUpdate={() => undefined} />
          </Card>
        )}

        <div className="mt-5 grid gap-4 lg:grid-cols-[1fr_300px]">
          <div className="space-y-3">
            {m.report && (
              <Card>
                <CardHeader
                  title="Relatório final"
                  icon={<FileText />}
                  action={
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        void navigator.clipboard
                          .writeText(m.report ?? "")
                          .then(() => toast.success("Relatório copiado (Markdown)"))
                          .catch(() => toast.error("Não foi possível copiar"))
                      }
                    >
                      <ClipboardCopy /> Copiar
                    </Button>
                  }
                />
                <div className="max-h-[420px] overflow-auto p-4">
                  <Markdown text={m.report} />
                </div>
              </Card>
            )}
            {tasks.length === 0 ? (
              <Card>
                <EmptyState
                  icon={<Sparkles />}
                  title={m.planning ? "O agente está planejando…" : "Sem plano ainda"}
                  description={
                    m.workspaceId
                      ? "Gere um plano com o agente Software Architect (somente leitura) ou crie as tarefas manualmente. Você revisa antes de iniciar."
                      : "Associe a missão a um workspace (botão Briefing) para planejar."
                  }
                />
              </Card>
            ) : (
              tasks.map((t) => <TaskCard key={t.id} detail={detail} task={t} act={act} />)
            )}
          </div>

          <div className="space-y-4">
            <Card>
              <CardHeader title="Progresso" />
              <div className="space-y-2 p-4 text-[12px]">
                <div className="h-1.5 overflow-hidden rounded-full bg-elevated">
                  <div className="h-full bg-success transition-all" style={{ width: tasks.length ? `${(done / tasks.length) * 100}%` : "0%" }} />
                </div>
                <div className="flex justify-between text-muted">
                  <span>
                    {done}/{tasks.length} concluídas
                  </span>
                  {needsAttention > 0 && <span className="text-warning">{needsAttention} pedem atenção</span>}
                </div>
                <dl className="mt-3 grid grid-cols-2 gap-y-1 text-[11.5px]">
                  <dt className="text-subtle">Execuções</dt>
                  <dd className="text-right text-muted">{totals.runs}</dd>
                  <dt className="text-subtle">Tokens</dt>
                  <dd className="text-right font-mono text-muted">
                    {fmtTokens(totals.inputTokens)} → {fmtTokens(totals.outputTokens)}
                  </dd>
                  <dt className="text-subtle">Tempo de agente</dt>
                  <dd className="text-right text-muted">{(totals.durationMs / 60000).toFixed(1)} min</dd>
                  <dt className="text-subtle">Custo informado</dt>
                  <dd className="text-right text-muted" title="Soma apenas dos valores informados pelos CLIs; nunca estimado.">
                    {totals.costUsd !== null ? `US$ ${totals.costUsd.toFixed(3)}` : "—"}
                    <span className="block text-[10px] text-subtle">
                      {totals.costReportedRuns} de {totals.runs} execuções informaram
                    </span>
                  </dd>
                </dl>
              </div>
            </Card>

            <MaestroConfig detail={detail} onSave={(st) => void run("Configuração", () => api.missionMaestro(id, st))} />

            <Card>
              <CardHeader title="Skills e MCP da missão" />
              <div className="p-4">
                <SkillChips kind="mission" targetId={id} editable={m.status !== "completed" && m.status !== "cancelled"} />
                <p className="mt-2 text-[11px] leading-relaxed text-subtle">Somadas às skills do agente e do squad em cada execução desta missão.</p>
                <div className="mt-3 mb-1 text-[10.5px] uppercase tracking-wider text-subtle">Servidores MCP</div>
                <McpChips kind="mission" targetId={id} editable={m.status !== "completed" && m.status !== "cancelled"} />
              </div>
            </Card>

            <Card>
              <CardHeader title="Briefing" />
              <p className="selectable max-h-48 overflow-auto p-4 text-[12px] whitespace-pre-wrap text-muted">{m.briefing}</p>
            </Card>

            <Card>
              <CardHeader title="Histórico" />
              <ul className="max-h-96 overflow-auto p-2">
                {events.length === 0 && <li className="px-2 py-2 text-[12px] text-subtle">Sem eventos.</li>}
                {events.map((e) => (
                  <li key={e.id} className="flex gap-2 px-2 py-1.5 text-[11.5px]">
                    <StatusDot tone={e.kind === "error" ? "error" : e.kind === "blocked" ? "warning" : e.kind === "complete" || e.kind === "integrate" ? "success" : "neutral"} />
                    <div className="min-w-0">
                      <p className="text-muted">{e.message}</p>
                      <p className="text-[10.5px] text-subtle">{relativeTime(e.createdAt)}</p>
                    </div>
                  </li>
                ))}
              </ul>
            </Card>
          </div>
        </div>
      </div>

      {editingPlan && <PlanEditor missionId={id} initial={toInputs(tasks)} open onClose={() => { setEditingPlan(false); load(); }} />}

      <BriefEditor open={editingBrief} detail={detail} onClose={() => { setEditingBrief(false); load(); void useMissions.getState().refresh(); }} />

      <Dialog open={!!feedback} onOpenChange={(o) => !o && setFeedback(null)}>
        {feedback && (
          <DialogContent
            title={feedback.action === "reject" ? `Rejeitar “${feedback.task.title}”?` : `Refazer “${feedback.task.title}”?`}
            description={feedback.action === "reject" ? "O feedback será enviado ao agente se você refizer a tarefa." : "A tarefa roda de novo numa worktree nova, com o feedback abaixo."}
          >
            <div className="space-y-3 px-5 py-4">
              <Textarea rows={4} autoFocus value={feedback.text} onChange={(e) => setFeedback({ ...feedback, text: e.target.value })} placeholder="O que precisa mudar? (opcional)" />
              <div className="flex justify-end gap-2">
                <Button variant="ghost" onClick={() => setFeedback(null)}>
                  Cancelar
                </Button>
                <Button
                  variant={feedback.action === "reject" ? "danger" : "primary"}
                  onClick={() => {
                    const f = feedback;
                    setFeedback(null);
                    void run("Ação na tarefa", () => api.missionTaskAction(id, f.task.id, f.action, f.text));
                  }}
                >
                  {feedback.action === "reject" ? "Rejeitar" : "Refazer"}
                </Button>
              </div>
            </div>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={saveRecipe !== null} onOpenChange={(o) => !o && setSaveRecipe(null)}>
        <DialogContent title="Salvar como receita" description="Briefing, squad, modo do Maestro, limites e o plano atual viram um modelo reutilizável.">
          <form
            className="space-y-3 px-5 py-4"
            onSubmit={(e) => {
              e.preventDefault();
              const name = (saveRecipe ?? "").trim();
              if (!name) return;
              setSaveRecipe(null);
              void run("Salvar receita", () => api.recipeFromMission(id, name).then(() => useLibrary.getState().refresh()), "Receita salva em Squads & Receitas");
            }}
          >
            <Input autoFocus value={saveRecipe ?? ""} onChange={(e) => setSaveRecipe(e.target.value)} aria-label="Nome da receita" />
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setSaveRecipe(null)}>
                Cancelar
              </Button>
              <Button type="submit" variant="primary">
                Salvar
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmCancel} onOpenChange={setConfirmCancel}>
        <DialogContent title="Cancelar a missão?" description={m.title}>
          <div className="px-5 py-4 text-[13px] text-muted">
            Agentes em execução são interrompidos e tarefas pendentes são canceladas. Worktrees e branches já criadas são mantidas.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmCancel(false)}>
                Voltar
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirmCancel(false);
                  void run("Cancelar missão", () => api.missionCancelRun(id), "Missão cancelada");
                }}
              >
                Cancelar missão
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function BriefEditor({ open, detail, onClose }: { open: boolean; detail: MissionDetail; onClose: () => void }) {
  const workspaces = useWorkspaces((s) => s.items);
  const [title, setTitle] = useState(detail.mission.title);
  const [briefing, setBriefing] = useState(detail.mission.briefing);
  const [ws, setWs] = useState(detail.mission.workspaceId ?? "");
  useEffect(() => {
    if (open) {
      setTitle(detail.mission.title);
      setBriefing(detail.mission.briefing);
      setWs(detail.mission.workspaceId ?? "");
    }
  }, [open, detail.mission]);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent title="Editar briefing">
        <form
          className="space-y-3 px-5 py-4"
          onSubmit={async (e) => {
            e.preventDefault();
            try {
              await api.missionUpdate(detail.mission.id, title, briefing, ws || null);
              onClose();
            } catch (err) {
              toast.error("Não foi possível salvar", { description: errorMessage(err) });
            }
          }}
        >
          <div>
            <Label htmlFor="mb-title">Nome</Label>
            <Input id="mb-title" value={title} onChange={(e) => setTitle(e.target.value)} />
          </div>
          <div>
            <Label htmlFor="mb-brief">Briefing</Label>
            <Textarea id="mb-brief" rows={8} value={briefing} onChange={(e) => setBriefing(e.target.value)} />
          </div>
          <div>
            <Label htmlFor="mb-ws">Workspace</Label>
            <select id="mb-ws" value={ws} onChange={(e) => setWs(e.target.value)} className="h-9 w-full rounded-md border border-border bg-bg-2 px-2 text-[13px] text-fg">
              <option value="">Sem workspace</option>
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancelar
            </Button>
            <Button type="submit" variant="primary">
              Salvar
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function MaestroConfig({ detail, onSave }: { detail: MissionDetail; onSave: (s: MaestroSettings) => void }) {
  const m = detail.mission;
  const locked = m.status === "completed" || m.status === "cancelled";
  const current: MaestroSettings = {
    mode: m.mode,
    maxParallel: m.maxParallel,
    requireReview: m.requireReview,
    maxAttempts: m.maxAttempts,
    maxRuns: m.maxRuns,
    budgetUsd: m.budgetUsd,
  };
  const [budget, setBudget] = useState(m.budgetUsd !== null ? String(m.budgetUsd) : "");
  useEffect(() => setBudget(m.budgetUsd !== null ? String(m.budgetUsd) : ""), [m.budgetUsd]);
  const save = (patch: Partial<MaestroSettings>) => onSave({ ...current, ...patch });

  return (
    <Card>
      <CardHeader title="Maestro" />
      <div className="space-y-3 p-4 text-[12px]">
        <div className="grid grid-cols-3 gap-1 rounded-md border border-border bg-bg-2 p-0.5" role="radiogroup" aria-label="Modo">
          {(Object.keys(MODE_INFO) as MissionMode[]).map((mode) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={m.mode === mode}
              disabled={locked}
              onClick={() => save({ mode })}
              className={cn("rounded px-2 py-1 text-[11.5px] font-medium", m.mode === mode ? "bg-elevated text-fg" : "text-subtle hover:text-muted")}
            >
              {MODE_INFO[mode].label}
            </button>
          ))}
        </div>
        <p className="text-[11px] leading-relaxed text-subtle">{MODE_INFO[m.mode].description}</p>
        <div className="flex items-center gap-2">
          <span className="flex-1 text-muted">Tarefas em paralelo</span>
          <select value={m.maxParallel} disabled={locked} onChange={(e) => save({ maxParallel: Number(e.target.value) })} className="h-7 rounded border border-border bg-bg-2 px-2 text-fg" aria-label="Tarefas em paralelo">
            {[1, 2, 3, 4].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </div>
        {m.mode === "free" && (
          <div className="flex items-center gap-2">
            <span className="flex-1 text-muted">Revisar cada tarefa</span>
            <Switch label="Revisar cada tarefa" checked={m.requireReview} disabled={locked} onCheckedChange={(v) => save({ requireReview: v })} />
          </div>
        )}
        {m.mode === "agentic" && (
          <div className="flex items-center gap-2">
            <span className="flex-1 text-muted">Tentativas por tarefa</span>
            <select value={m.maxAttempts} disabled={locked} onChange={(e) => save({ maxAttempts: Number(e.target.value) })} className="h-7 rounded border border-border bg-bg-2 px-2 text-fg" aria-label="Tentativas por tarefa">
              {[1, 2, 3, 4, 5].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </div>
        )}
        <div className="flex items-center gap-2">
          <span className="flex-1 text-muted" title="Escolhe o modelo do Claude (haiku, sonnet ou opus) pelo porte de cada tarefa quando o perfil não fixa um modelo. O motivo fica no histórico da missão.">
            Harness: modelo por tarefa
          </span>
          <Switch
            label="Harness: modelo por tarefa"
            checked={m.harness}
            disabled={locked}
            onCheckedChange={(v) =>
              void api
                .missionSetHarness(m.id, v)
                .then(() => onSave(current))
                .catch((e) => toast.error("Falha ao salvar", { description: errorMessage(e) }))
            }
          />
        </div>
        <div className="flex items-center gap-2">
          <span className="flex-1 text-muted">Máx. de execuções</span>
          <select value={m.maxRuns} disabled={locked} onChange={(e) => save({ maxRuns: Number(e.target.value) })} className="h-7 rounded border border-border bg-bg-2 px-2 text-fg" aria-label="Máximo de execuções">
            {[10, 20, 40, 80, 150].concat([m.maxRuns]).filter((v, i, a) => a.indexOf(v) === i).sort((a, b) => a - b).map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </div>
        <div className="flex items-center gap-2">
          <span className="flex-1 text-muted">Orçamento (US$)</span>
          <Input
            className="h-7 w-24 text-right text-[12px]"
            inputMode="decimal"
            value={budget}
            disabled={locked}
            placeholder="sem limite"
            onChange={(e) => setBudget(e.target.value)}
            onBlur={() => {
              const v = budget.trim() ? Number(budget.replace(",", ".")) : null;
              if (v !== null && !(v > 0)) {
                toast.error("Orçamento inválido");
                return;
              }
              if (v !== m.budgetUsd) save({ budgetUsd: v });
            }}
          />
        </div>
        <p className="text-[11px] leading-relaxed text-subtle">
          O orçamento considera apenas custos informados pelos CLIs. Ao atingir um limite, o Maestro pausa a missão. Aprovar o plano e integrar na sua branch são sempre decisões suas. Cada tarefa roda em sua própria worktree.
        </p>
      </div>
    </Card>
  );
}
