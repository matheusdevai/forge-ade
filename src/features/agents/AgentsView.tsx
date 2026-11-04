import { useEffect, useState } from "react";
import { Bot, Copy, Plus, RotateCcw, Save, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, CardHeader, Dialog, DialogContent, EmptyState, Input, Label, StatusDot, Textarea } from "@/components/ui";
import { openWorkspace } from "@/features/workspace/actions";
import { api, errorMessage, type AgentProfile, type AgentRun, type Permission, type ProfileInput, type ProviderId } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useWorkspaces } from "@/stores/data";
import { PERMISSION_INFO, useAgents } from "@/stores/agents";
import { useProviders } from "@/stores/providers";
import { SkillChips } from "@/features/skills/SkillChips";
import { McpChips } from "@/features/mcp/McpChips";

const EMPTY: ProfileInput = {
  name: "Novo agente",
  role: "custom",
  description: "",
  providerId: "claude",
  model: null,
  instructions: "",
  permission: "read",
  maxTurns: null,
};

const toInput = (p: AgentProfile): ProfileInput => ({
  name: p.name,
  role: p.role,
  description: p.description,
  providerId: p.providerId,
  model: p.model,
  instructions: p.instructions,
  permission: p.permission,
  maxTurns: p.maxTurns,
});

function ProfileEditor({ profile, onSaved, onDeleted }: { profile: AgentProfile | null; onSaved: (p: AgentProfile) => void; onDeleted: () => void }) {
  const { create, update, remove, restore } = useAgents.getState();
  const providers = useProviders((s) => s.items);
  const [form, setForm] = useState<ProfileInput>(profile ? toInput(profile) : EMPTY);
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => setForm(profile ? toInput(profile) : EMPTY), [profile]);

  const set = <K extends keyof ProfileInput>(k: K, v: ProfileInput[K]) => setForm((f) => ({ ...f, [k]: v }));
  const provider = providers.find((p) => p.id === form.providerId);
  const dirty = profile ? JSON.stringify(toInput(profile)) !== JSON.stringify(form) : true;

  const save = async () => {
    setSaving(true);
    try {
      const p = profile ? await update(profile.id, form) : await create(form);
      toast.success("Perfil salvo");
      onSaved(p);
    } catch (e) {
      toast.error("Não foi possível salvar", { description: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3">
        <div>
          <Label htmlFor="pf-name">Nome</Label>
          <Input id="pf-name" value={form.name} onChange={(e) => set("name", e.target.value)} />
        </div>
        <div>
          <Label htmlFor="pf-role">Função</Label>
          <Input id="pf-role" value={form.role} onChange={(e) => set("role", e.target.value)} />
        </div>
      </div>
      <div>
        <Label htmlFor="pf-desc">Descrição</Label>
        <Input id="pf-desc" value={form.description} onChange={(e) => set("description", e.target.value)} />
      </div>
      <div className="grid grid-cols-3 gap-3">
        <div>
          <Label htmlFor="pf-provider">Provider</Label>
          <select
            id="pf-provider"
            value={form.providerId}
            onChange={(e) => set("providerId", e.target.value as ProviderId)}
            className="h-9 w-full rounded-md border border-border bg-bg-2 px-2 text-[13px] text-fg focus:outline-none"
          >
            {providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
                {!p.installed ? " (não instalado)" : p.auth.state !== "logged_in" ? " (sem login)" : ""}
              </option>
            ))}
          </select>
        </div>
        <div>
          <Label htmlFor="pf-model">Modelo</Label>
          <Input
            id="pf-model"
            list="pf-models"
            value={form.model ?? ""}
            placeholder="padrão do CLI"
            onChange={(e) => set("model", e.target.value || null)}
            className="font-mono text-[12px]"
          />
          <datalist id="pf-models">
            {provider?.modelAliases.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </div>
        <div>
          <Label htmlFor="pf-turns">Máx. de turnos</Label>
          <Input
            id="pf-turns"
            type="number"
            min={1}
            max={500}
            value={form.maxTurns ?? ""}
            placeholder={form.providerId === "claude" ? "sem limite" : "n/d neste CLI"}
            disabled={form.providerId !== "claude"}
            onChange={(e) => set("maxTurns", e.target.value ? Number(e.target.value) : null)}
          />
        </div>
      </div>
      <div>
        <Label>Permissão padrão</Label>
        <div className="grid grid-cols-3 gap-2">
          {(Object.keys(PERMISSION_INFO) as Permission[]).map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => set("permission", p)}
              className={cn(
                "rounded-md border p-2 text-left",
                form.permission === p ? (p === "full" ? "border-error/60 bg-error/10" : "border-accent/60 bg-accent/10") : "border-border hover:border-border-strong",
              )}
            >
              <div className="text-[12px] font-medium text-fg">{PERMISSION_INFO[p].label}</div>
              <div className="mt-0.5 text-[11px] leading-snug text-muted">{PERMISSION_INFO[p].description}</div>
            </button>
          ))}
        </div>
      </div>
      <div>
        <Label htmlFor="pf-instr">Instruções</Label>
        <Textarea id="pf-instr" rows={8} value={form.instructions} onChange={(e) => set("instructions", e.target.value)} className="font-mono text-[12px]" />
        <p className="mt-1 text-[11px] text-subtle">
          {form.providerId === "claude"
            ? "Enviadas como complemento do system prompt (--append-system-prompt)."
            : "Este CLI não tem flag de system prompt: as instruções são enviadas como prefixo da mensagem."}
        </p>
      </div>
      {profile && (
        <div>
          <Label>Skills deste agente</Label>
          <SkillChips kind="profile" targetId={profile.id} />
          <Label className="mt-3">Servidores MCP (Claude Code)</Label>
          <McpChips kind="profile" targetId={profile.id} />
        </div>
      )}
      <div className="flex items-center gap-2">
        {profile && !profile.builtin && (
          <Button variant="ghost" onClick={() => setConfirmDelete(true)}>
            <Trash2 /> Excluir
          </Button>
        )}
        {profile?.builtin && (
          <Button
            variant="ghost"
            onClick={() =>
              void restore(profile.id)
                .then((p) => {
                  toast.success("Perfil padrão restaurado");
                  onSaved(p);
                })
                .catch((e) => toast.error("Falha", { description: errorMessage(e) }))
            }
          >
            <RotateCcw /> Restaurar padrão
          </Button>
        )}
        {profile && (
          <Button variant="ghost" onClick={() => void create({ ...form, name: `${form.name} (cópia)` }).then(onSaved).catch((e) => toast.error("Falha", { description: errorMessage(e) }))}>
            <Copy /> Duplicar
          </Button>
        )}
        <Button variant="primary" className="ml-auto" disabled={!dirty || saving} onClick={() => void save()}>
          <Save /> Salvar
        </Button>
      </div>
      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent title="Excluir perfil?" description={profile?.name}>
          <div className="px-5 py-4 text-[13px] text-muted">
            O histórico de execuções é mantido.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirmDelete(false);
                  if (profile) void remove(profile.id).then(onDeleted).catch((e) => toast.error("Falha", { description: errorMessage(e) }));
                }}
              >
                Excluir
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

const STATUS_TONE = { running: "accent", completed: "success", failed: "error", cancelled: "neutral" } as const;

function RecentRuns() {
  const [runs, setRuns] = useState<AgentRun[] | null>(null);
  const workspaces = useWorkspaces((s) => s.items);
  const profiles = useAgents((s) => s.profiles);
  const providers = useProviders((s) => s.items);
  useEffect(() => {
    let alive = true;
    const load = () => api.agentRunsList(null, null, 30).then((r) => alive && setRuns(r)).catch(() => alive && setRuns([]));
    void load();
    const t = setInterval(load, 5000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  return (
    <Card className="mt-6">
      <CardHeader title="Execuções recentes" icon={<Bot />} />
      {!runs || runs.length === 0 ? (
        <EmptyState title="Nenhuma execução ainda" description="Abra um pane Agente num workspace (paleta ⌘K → Agente)." />
      ) : (
        <ul className="divide-y divide-border">
          {runs.map((r) => {
            const ws = workspaces.find((w) => w.id === r.workspaceId);
            return (
              <li key={r.id} className="flex items-center gap-3 px-4 py-2 text-[12px]">
                <StatusDot tone={STATUS_TONE[r.status]} pulse={r.status === "running"} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-fg">{r.prompt}</div>
                  <div className="truncate text-[11px] text-subtle">
                    {profiles.find((p) => p.id === r.profileId)?.name ?? "—"} · {providers.find((p) => p.id === r.providerId)?.name ?? r.providerId} · {PERMISSION_INFO[r.permission].short}
                    {r.worktreeBranch ? ` · ⎇ ${r.worktreeBranch}` : ""} · {relativeTime(r.startedAt)}
                  </div>
                </div>
                <span className="shrink-0 text-right font-mono text-[10.5px] text-subtle">
                  {r.durationMs !== null ? `${(r.durationMs / 1000).toFixed(1)} s` : ""}
                  {r.outputTokens !== null ? ` · ${r.outputTokens} tok` : ""}
                </span>
                {ws && (
                  <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => void openWorkspace(ws.id)}>
                    {ws.name}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

export function AgentsView() {
  const { profiles, loaded, refresh } = useAgents();
  const providersLoaded = useProviders((s) => s.loaded);
  const [selected, setSelected] = useState<string | "new" | null>(null);
  useEffect(() => {
    void refresh().catch((e) => toast.error("Falha ao carregar agentes", { description: errorMessage(e) }));
    if (!providersLoaded) void useProviders.getState().refresh().catch(() => undefined);
  }, [refresh, providersLoaded]);
  useEffect(() => {
    if (!selected && profiles[0]) setSelected(profiles[0].id);
  }, [profiles, selected]);

  const current = selected === "new" ? null : (profiles.find((p) => p.id === selected) ?? null);

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-6 flex items-end">
          <div>
            <h1 className="text-xl font-semibold">Agentes</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Perfis reutilizáveis: provider, modelo, instruções, permissão e limites. Use-os no pane <b className="text-fg">Agente</b> de qualquer workspace. Skills e memória por agente chegam nos milestones 11 e 14.
            </p>
          </div>
          <Button variant="primary" className="ml-auto" onClick={() => setSelected("new")}>
            <Plus /> Novo perfil
          </Button>
        </div>
        {!loaded ? null : (
          <div className="grid gap-4 lg:grid-cols-[280px_1fr]">
            <Card className="h-fit overflow-hidden">
              <ul className="p-1.5">
                {profiles.map((p) => (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => setSelected(p.id)}
                      className={cn("flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left", selected === p.id ? "bg-accent/15" : "hover:bg-elevated")}
                    >
                      <Bot className={cn("size-3.5 shrink-0", selected === p.id ? "text-accent" : "text-subtle")} />
                      <span className="min-w-0 flex-1 truncate text-[12.5px] text-fg">{p.name}</span>
                      <span className="shrink-0 text-[10.5px] text-subtle">{PERMISSION_INFO[p.permission].short}</span>
                      {p.origin === "import" ? <Badge tone="warning">importado</Badge> : !p.builtin && <Badge>custom</Badge>}
                    </button>
                  </li>
                ))}
              </ul>
            </Card>
            <Card className="p-5">
              {selected === null ? (
                <EmptyState title="Selecione um perfil" />
              ) : (
                <ProfileEditor
                  key={selected}
                  profile={current}
                  onSaved={(p) => setSelected(p.id)}
                  onDeleted={() => setSelected(profiles.find((p) => p.id !== selected)?.id ?? null)}
                />
              )}
            </Card>
          </div>
        )}
        <RecentRuns />
      </div>
    </div>
  );
}
