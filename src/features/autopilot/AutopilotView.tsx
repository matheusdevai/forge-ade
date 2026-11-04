import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { History, Loader2, Pencil, Play, Plus, ShieldAlert, Square, Timer, Trash2, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, Dialog, DialogContent, EmptyState, Input, Label, Switch, Textarea } from "@/components/ui";
import { api, errorMessage, type AutopilotEvent, type DevScript, type Routine, type RoutineAction, type RoutineInput, type RoutineRun } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useAgents } from "@/stores/agents";
import { useApp } from "@/stores/app";
import { useWorkspaces } from "@/stores/data";
import { useLibrary } from "@/stores/library";
import { DAYS, actionLabel, actionSummary, duration, triggerSummary } from "./model";

const select = "h-8 rounded-md border border-border bg-bg px-2 text-[12.5px] text-fg outline-none focus:border-accent/60";

const RUN_TONE: Record<RoutineRun["status"], "success" | "error" | "warning" | "neutral" | "accent"> = {
  running: "accent",
  succeeded: "success",
  failed: "error",
  skipped: "warning",
  cancelled: "neutral",
};
const RUN_LABEL: Record<RoutineRun["status"], string> = {
  running: "rodando",
  succeeded: "ok",
  failed: "falhou",
  skipped: "pulada",
  cancelled: "cancelada",
};

function defaultAction(type: RoutineAction["type"], profileId: string, recipeId: string): RoutineAction {
  switch (type) {
    case "agent":
      return { type, profileId, prompt: "", permission: "read", useWorktree: false };
    case "script":
      return { type, script: "" };
    case "command":
      return { type, command: "" };
    case "mission":
      return { type, recipeId, title: "", values: {} };
    case "verify":
      return { type, url: "http://localhost:3000" };
  }
}

function toInput(r: Routine): RoutineInput {
  return {
    name: r.name,
    workspaceId: r.workspaceId,
    action: r.action,
    trigger: r.trigger,
    intervalMin: r.intervalMin,
    dailyTime: r.dailyTime,
    daysMask: r.daysMask,
    maxRunsPerDay: r.maxRunsPerDay,
    timeoutMin: r.timeoutMin,
    maxFailures: r.maxFailures,
    budgetUsdDay: r.budgetUsdDay,
    approveCommand: false,
  };
}

function Editor({ editing, onClose, onSaved }: { editing: Routine | null; onClose: () => void; onSaved: () => void }) {
  const workspaces = useWorkspaces((s) => s.items);
  const profiles = useAgents((s) => s.profiles);
  const recipes = useLibrary((s) => s.recipes);
  const [v, setV] = useState<RoutineInput>(() =>
    editing
      ? toInput(editing)
      : {
          name: "",
          workspaceId: workspaces[0]?.id ?? "",
          action: defaultAction("agent", profiles[0]?.id ?? "", recipes[0]?.id ?? ""),
          trigger: "daily",
          intervalMin: 60,
          dailyTime: "09:00",
          daysMask: 0b0011111,
          maxRunsPerDay: 24,
          timeoutMin: 30,
          maxFailures: 3,
          budgetUsdDay: null,
          approveCommand: false,
        },
  );
  const [scripts, setScripts] = useState<DevScript[]>([]);
  const [saving, setSaving] = useState(false);
  const set = (p: Partial<RoutineInput>) => setV((c) => ({ ...c, ...p }));
  const setAction = (p: Partial<RoutineAction>) => setV((c) => ({ ...c, action: { ...c.action, ...p } as RoutineAction }));

  useEffect(() => {
    if (!v.workspaceId) return;
    api
      .previewDetect(v.workspaceId)
      .then((i) => setScripts(i.scripts))
      .catch(() => setScripts([]));
  }, [v.workspaceId]);

  const a = v.action;
  const commandChanged = a.type === "command" && !(editing?.action.type === "command" && editing.action.command === a.command && editing.commandApproved);
  const needsApproval = a.type === "command" && commandChanged;
  const ws = workspaces.find((w) => w.id === v.workspaceId);
  const recipe = a.type === "mission" ? recipes.find((r) => r.id === a.recipeId) : undefined;

  const save = async () => {
    setSaving(true);
    try {
      if (editing) await api.autopilotUpdate(editing.id, v);
      else await api.autopilotCreate(v);
      toast.success(editing ? "Rotina atualizada" : "Rotina criada");
      onSaved();
    } catch (e) {
      toast.error("Não foi possível salvar", { description: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={editing ? "Editar rotina" : "Nova rotina"} description="Rotinas rodam enquanto o FORGE está aberto." className="w-[min(720px,calc(100vw-32px))]">
        <div className="max-h-[68vh] space-y-3 overflow-auto px-5 py-4 text-[12.5px]">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label>Nome</Label>
              <Input value={v.name} onChange={(e) => set({ name: e.target.value })} placeholder="Ex.: Revisão diária do código" maxLength={80} />
            </div>
            <div className="space-y-1">
              <Label>Workspace</Label>
              <select className={cn(select, "w-full")} value={v.workspaceId} onChange={(e) => set({ workspaceId: e.target.value })}>
                {workspaces.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="space-y-1">
            <Label>Ação</Label>
            <div className="flex flex-wrap gap-1">
              {(["agent", "script", "command", "mission", "verify"] as const).map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => set({ action: a.type === t ? a : defaultAction(t, profiles[0]?.id ?? "", recipes[0]?.id ?? "") })}
                  className={cn("rounded-md border px-2.5 py-1 text-[12px]", a.type === t ? "border-accent/60 bg-accent/10 text-accent" : "border-border text-muted hover:text-fg")}
                >
                  {actionLabel(t)}
                </button>
              ))}
            </div>
          </div>

          {a.type === "agent" && (
            <div className="space-y-2">
              <div className="grid gap-3 sm:grid-cols-2">
                <select className={select} value={a.profileId} onChange={(e) => setAction({ profileId: e.target.value })}>
                  {profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
                <select className={select} value={a.permission} onChange={(e) => setAction({ permission: e.target.value as "read" | "edit" })}>
                  <option value="read">Somente leitura</option>
                  <option value="edit">Pode editar</option>
                </select>
              </div>
              <Textarea rows={4} value={a.prompt} onChange={(e) => setAction({ prompt: e.target.value })} placeholder="O que o agente deve fazer a cada execução…" />
              <label className="flex items-center gap-2 text-muted">
                <input type="checkbox" checked={a.useWorktree} onChange={(e) => setAction({ useWorktree: e.target.checked })} /> Rodar numa worktree isolada (recomendado com edição)
              </label>
              <p className="text-[11px] text-subtle">Rotinas nunca usam “acesso total”.</p>
            </div>
          )}
          {a.type === "script" && (
            <select className={cn(select, "w-full")} value={a.script} onChange={(e) => setAction({ script: e.target.value })}>
              <option value="">Escolha um script do package.json…</option>
              {scripts.map((s) => (
                <option key={s.name} value={s.name}>
                  {s.name} — {s.command}
                </option>
              ))}
            </select>
          )}
          {a.type === "command" && (
            <div className="space-y-2">
              <Textarea rows={3} className="font-mono text-[12px]" value={a.command} onChange={(e) => setAction({ command: e.target.value })} placeholder="npm test && npm run lint" />
              {needsApproval ? (
                <label className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/8 px-3 py-2 text-[12px] text-warning">
                  <input type="checkbox" className="mt-0.5" checked={v.approveCommand} onChange={(e) => set({ approveCommand: e.target.checked })} />
                  <span>
                    Aprovo que o FORGE execute exatamente este comando, no shell, dentro de <b>{ws?.name ?? "workspace"}</b>, sempre que a rotina disparar. Qualquer edição exigirá nova aprovação.
                  </span>
                </label>
              ) : (
                <p className="flex items-center gap-1.5 text-[11.5px] text-success">
                  <ShieldAlert className="size-3.5" /> Comando aprovado.
                </p>
              )}
            </div>
          )}
          {a.type === "mission" && (
            <div className="space-y-2">
              <select className={cn(select, "w-full")} value={a.recipeId} onChange={(e) => setAction({ recipeId: e.target.value, values: {} })}>
                {recipes.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.name}
                  </option>
                ))}
              </select>
              <Input value={a.title} onChange={(e) => setAction({ title: e.target.value })} placeholder="Título das missões criadas (data e hora são acrescentadas)" />
              {recipe?.fields.map((f) => (
                <div key={f.key} className="space-y-1">
                  <Label>
                    {f.label}
                    {f.required ? " *" : ""}
                  </Label>
                  <Input value={a.values[f.key] ?? ""} placeholder={f.placeholder} onChange={(e) => setAction({ values: { ...a.values, [f.key]: e.target.value } })} />
                </div>
              ))}
              <p className="text-[11px] text-subtle">A rotina cria a missão e inicia o planejamento. Executar o plano continua dependendo da sua aprovação.</p>
            </div>
          )}
          {a.type === "verify" && (
            <div className="space-y-1">
              <Input value={a.url} onChange={(e) => setAction({ url: e.target.value })} placeholder="http://localhost:3000" />
              <p className="text-[11px] text-subtle">Abre a página no navegador do FORGE, recarrega e falha se houver erros de console, exceções ou falhas de rede. O servidor precisa estar rodando.</p>
            </div>
          )}

          <div className="space-y-1 border-t border-border pt-3">
            <Label>Quando</Label>
            <div className="flex flex-wrap items-center gap-2">
              <select className={select} value={v.trigger} onChange={(e) => set({ trigger: e.target.value as RoutineInput["trigger"] })}>
                <option value="daily">Diariamente</option>
                <option value="interval">A cada intervalo</option>
                <option value="commit">A cada novo commit</option>
                <option value="manual">Só manualmente</option>
              </select>
              {v.trigger === "interval" && (
                <>
                  <Input type="number" min={5} max={10080} className="h-8 w-24" value={v.intervalMin ?? 60} onChange={(e) => set({ intervalMin: Number(e.target.value) })} />
                  <span className="text-muted">minutos (mín. 5)</span>
                </>
              )}
              {v.trigger === "daily" && (
                <>
                  <Input type="time" className="h-8 w-28" value={v.dailyTime ?? "09:00"} onChange={(e) => set({ dailyTime: e.target.value })} />
                  <div className="flex gap-0.5">
                    {DAYS.map((d, i) => (
                      <button
                        key={d}
                        type="button"
                        onClick={() => set({ daysMask: v.daysMask ^ (1 << i) })}
                        className={cn("rounded px-1.5 py-0.5 text-[11px]", v.daysMask & (1 << i) ? "bg-accent/15 text-accent" : "text-subtle hover:text-muted")}
                      >
                        {d}
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>
          </div>

          <div className="grid gap-3 border-t border-border pt-3 sm:grid-cols-4">
            <div className="space-y-1">
              <Label>Máx. por dia</Label>
              <Input type="number" min={1} max={288} className="h-8" value={v.maxRunsPerDay} onChange={(e) => set({ maxRunsPerDay: Number(e.target.value) })} />
            </div>
            <div className="space-y-1">
              <Label>Tempo máx. (min)</Label>
              <Input type="number" min={1} max={240} className="h-8" value={v.timeoutMin} onChange={(e) => set({ timeoutMin: Number(e.target.value) })} />
            </div>
            <div className="space-y-1">
              <Label>Desativar após falhas</Label>
              <Input type="number" min={1} max={20} className="h-8" value={v.maxFailures} onChange={(e) => set({ maxFailures: Number(e.target.value) })} />
            </div>
            <div className="space-y-1">
              <Label>Orçamento/dia (US$)</Label>
              <Input
                className="h-8"
                inputMode="decimal"
                placeholder="sem limite"
                value={v.budgetUsdDay ?? ""}
                onChange={(e) => set({ budgetUsdDay: e.target.value.trim() ? Number(e.target.value.replace(",", ".")) : null })}
              />
            </div>
          </div>
          <p className="text-[11px] text-subtle">O orçamento conta só o custo informado pelo CLI do agente; execuções sem custo informado não são estimadas.</p>
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="primary" disabled={saving || !v.name.trim() || !v.workspaceId || (needsApproval && !v.approveCommand)} onClick={() => void save()}>
            Salvar
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function RunsPanel({ routine, onClose }: { routine: Routine; onClose: () => void }) {
  const [runs, setRuns] = useState<RoutineRun[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const navigate = useApp((s) => s.navigate);
  const load = useCallback(() => void api.autopilotRuns(routine.id, 100).then(setRuns).catch(() => setRuns([])), [routine.id]);
  useEffect(() => {
    load();
    const un = listen<AutopilotEvent>("forge://autopilot-changed", (e) => e.payload.routineId === routine.id && load());
    return () => void un.then((f) => f());
  }, [load, routine.id]);
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={`Histórico — ${routine.name}`} description={actionSummary(routine.action)} className="w-[min(760px,calc(100vw-32px))]">
        <div className="max-h-[65vh] overflow-auto px-5 py-3">
          {!runs ? (
            <Loader2 className="size-4 animate-spin text-muted" />
          ) : runs.length === 0 ? (
            <p className="py-4 text-xs text-subtle">Nenhuma execução ainda.</p>
          ) : (
            runs.map((r) => (
              <div key={r.id} className="border-b border-border/60 py-2 text-[12px]">
                <button className="flex w-full items-center gap-2 text-left" onClick={() => setOpen(open === r.id ? null : r.id)}>
                  <Badge tone={RUN_TONE[r.status]}>{RUN_LABEL[r.status]}</Badge>
                  <span className="text-muted">{r.trigger === "schedule" ? "agendada" : r.trigger === "commit" ? "commit" : "manual"}</span>
                  <span className="text-subtle">{relativeTime(r.startedAt)}</span>
                  <span className="text-subtle">· {duration(r.startedAt, r.endedAt)}</span>
                  {r.costUsd !== null && <span className="text-subtle">· US$ {r.costUsd.toFixed(4)} informado</span>}
                  {r.error && <span className="ml-auto max-w-[45%] truncate text-error">{r.error}</span>}
                </button>
                {open === r.id && (
                  <div className="mt-2 space-y-2">
                    {r.status === "running" && (
                      <Button size="sm" variant="danger" onClick={() => void api.autopilotCancel(r.id)}>
                        <Square /> Cancelar
                      </Button>
                    )}
                    {r.output && <pre className="max-h-64 overflow-auto rounded bg-bg p-2 font-mono text-[11px] whitespace-pre-wrap text-muted">{r.output}</pre>}
                    {r.missionId && (
                      <Button size="sm" variant="ghost" onClick={() => navigate({ view: "mission", id: r.missionId! })}>
                        Abrir missão
                      </Button>
                    )}
                    {r.agentRunId && <p className="text-[11px] text-subtle">Execução do agente registrada em Agentes → histórico.</p>}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function AutopilotView() {
  const workspaces = useWorkspaces((s) => s.items);
  const [routines, setRoutines] = useState<Routine[] | null>(null);
  const [last, setLast] = useState<Record<string, RoutineRun>>({});
  const [editor, setEditor] = useState<{ editing: Routine | null } | null>(null);
  const [history, setHistory] = useState<Routine | null>(null);
  const [deleting, setDeleting] = useState<Routine | null>(null);

  const load = useCallback(() => {
    api
      .autopilotList(null)
      .then(setRoutines)
      .catch((e) => toast.error("Falha ao carregar rotinas", { description: errorMessage(e) }));
    api
      .autopilotRuns(null, 200)
      .then((runs) => {
        const m: Record<string, RoutineRun> = {};
        for (const r of runs) if (!m[r.routineId]) m[r.routineId] = r;
        setLast(m);
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    load();
    if (!useWorkspaces.getState().loaded) void useWorkspaces.getState().refresh().catch(() => undefined);
    if (!useAgents.getState().loaded) void useAgents.getState().refresh().catch(() => undefined);
    if (!useLibrary.getState().loaded) void useLibrary.getState().refresh().catch(() => undefined);
    const un = listen<AutopilotEvent>("forge://autopilot-changed", () => load());
    const t = setInterval(load, 30_000);
    return () => {
      void un.then((f) => f());
      clearInterval(t);
    };
  }, [load]);

  const act = (p: Promise<unknown>, ok?: string) =>
    void p
      .then(() => {
        if (ok) toast.success(ok);
        load();
      })
      .catch((e) => toast.error("Falha", { description: errorMessage(e) }));

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-5xl px-8 py-8">
        <div className="mb-5 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Autopilot</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Rotinas automáticas: agentes, scripts, comandos aprovados, missões de receitas e verificação de páginas. Rodam enquanto o FORGE está aberto; execuções
              perdidas com o app fechado não são repetidas. Limites diários, tempo máximo e desativação após falhas evitam execução descontrolada.
            </p>
          </div>
          <Button className="ml-auto" variant="primary" disabled={workspaces.length === 0} onClick={() => setEditor({ editing: null })}>
            <Plus /> Nova rotina
          </Button>
        </div>
        {!routines ? null : routines.length === 0 ? (
          <Card>
            <EmptyState
              icon={<Timer />}
              title="Nenhuma rotina"
              description={workspaces.length === 0 ? "Abra um workspace primeiro." : "Ex.: revisão de código toda manhã, testes a cada commit, verificação da página a cada hora."}
              action={
                workspaces.length > 0 ? (
                  <Button variant="primary" onClick={() => setEditor({ editing: null })}>
                    <Plus /> Nova rotina
                  </Button>
                ) : undefined
              }
            />
          </Card>
        ) : (
          <div className="space-y-2">
            {routines.map((r) => {
              const lr = last[r.id];
              return (
                <Card key={r.id} className={cn("px-4 py-3", !r.enabled && "opacity-75")}>
                  <div className="flex flex-wrap items-center gap-2">
                    <Switch label="Ativa" checked={r.enabled} onCheckedChange={(on) => act(api.autopilotSetEnabled(r.id, on))} />
                    <span className="text-[13.5px] font-medium">{r.name}</span>
                    <Badge>{actionLabel(r.action.type)}</Badge>
                    <span className="text-[11.5px] text-subtle">{workspaces.find((w) => w.id === r.workspaceId)?.name}</span>
                    <div className="ml-auto flex gap-1">
                      <Button size="sm" variant="ghost" onClick={() => act(api.autopilotRunNow(r.id).then((run) => run.status === "skipped" && toast.warning("Execução pulada", { description: run.error ?? undefined })))}>
                        <Play /> Executar agora
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => setHistory(r)}>
                        <History /> Histórico
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => setEditor({ editing: r })} aria-label="Editar">
                        <Pencil />
                      </Button>
                      <Button size="sm" variant="ghost" className="text-error" onClick={() => setDeleting(r)} aria-label="Excluir">
                        <Trash2 />
                      </Button>
                    </div>
                  </div>
                  <p className="mt-1 truncate text-[12px] text-muted" title={actionSummary(r.action)}>
                    {actionSummary(r.action)}
                  </p>
                  <div className="mt-1.5 flex flex-wrap items-center gap-3 text-[11.5px] text-subtle">
                    <span>{triggerSummary(r)}</span>
                    {r.enabled && r.nextRunAt && <span>próxima: {new Date(r.nextRunAt).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" })}</span>}
                    {lr && (
                      <span className="flex items-center gap-1">
                        última: <Badge tone={RUN_TONE[lr.status]}>{RUN_LABEL[lr.status]}</Badge> {relativeTime(lr.startedAt)}
                      </span>
                    )}
                    {r.action.type === "command" && !r.commandApproved && <span className="text-warning">comando precisa de nova aprovação</span>}
                  </div>
                  {r.disabledReason && (
                    <p className="mt-1.5 flex items-center gap-1.5 text-[11.5px] text-warning">
                      <TriangleAlert className="size-3.5" /> {r.disabledReason} — corrija e reative.
                    </p>
                  )}
                </Card>
              );
            })}
          </div>
        )}
      </div>
      {editor && (
        <Editor
          editing={editor.editing}
          onClose={() => setEditor(null)}
          onSaved={() => {
            setEditor(null);
            load();
          }}
        />
      )}
      {history && <RunsPanel routine={history} onClose={() => setHistory(null)} />}
      <Dialog open={!!deleting} onOpenChange={(o) => !o && setDeleting(null)}>
        {deleting && (
          <DialogContent title="Excluir rotina?" description={`“${deleting.name}” e o histórico de execuções serão apagados.`}>
            <div className="flex justify-end gap-2 px-5 py-4">
              <Button variant="ghost" onClick={() => setDeleting(null)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  const id = deleting.id;
                  setDeleting(null);
                  act(api.autopilotDelete(id), "Rotina excluída");
                }}
              >
                Excluir
              </Button>
            </div>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
