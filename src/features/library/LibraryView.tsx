import { useEffect, useState } from "react";
import { BookOpen, Copy, Download, Play, Plus, RotateCcw, Save, ShieldAlert, Trash2, Upload, Users } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, Dialog, DialogContent, EmptyState, Input, Label, Switch, Textarea } from "@/components/ui";
import { MODE_INFO } from "@/features/missions/MissionStatusBadge";
import { api, errorMessage, type BundlePreview, type MissionMode, type Recipe, type RecipeField, type RecipeInput, type Squad, type SquadInput } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { PERMISSION_INFO, useAgents } from "@/stores/agents";
import { useLibrary } from "@/stores/library";
import { UseRecipeDialog } from "./UseRecipeDialog";
import { SkillChips } from "@/features/skills/SkillChips";
import { McpChips } from "@/features/mcp/McpChips";

const select = "h-8 rounded-md border border-border bg-bg-2 px-2 text-[12.5px] text-fg focus:outline-none";

function OriginBadge({ origin }: { origin: string }) {
  if (origin === "import") return <Badge tone="warning">importado</Badge>;
  if (origin === "local") return <Badge>custom</Badge>;
  return null;
}

async function exportBundle(kind: "squad" | "recipe", id: string) {
  try {
    const path = await api.bundleExport(kind, id);
    if (path) toast.success("Exportado", { description: path });
  } catch (e) {
    toast.error("Falha ao exportar", { description: errorMessage(e) });
  }
}

// ── Squads ────────────────────────────────────────────────────────────────

const toSquadInput = (s: Squad): SquadInput => ({ name: s.name, description: s.description, members: s.members, rules: s.rules, flow: s.flow, defaultMode: s.defaultMode });

function SquadEditor({ squad, onSaved, onDeleted }: { squad: Squad | null; onSaved: (id: string) => void; onDeleted: () => void }) {
  const profiles = useAgents((s) => s.profiles);
  const refresh = useLibrary((s) => s.refresh);
  const empty: SquadInput = { name: "Novo squad", description: "", members: [], rules: "", flow: "", defaultMode: "squad" };
  const [form, setForm] = useState<SquadInput>(squad ? toSquadInput(squad) : empty);
  useEffect(() => setForm(squad ? toSquadInput(squad) : empty), [squad]); // eslint-disable-line react-hooks/exhaustive-deps
  const set = <K extends keyof SquadInput>(k: K, v: SquadInput[K]) => setForm((f) => ({ ...f, [k]: v }));
  const dirty = squad ? JSON.stringify(toSquadInput(squad)) !== JSON.stringify(form) : true;

  const act = async (fn: () => Promise<unknown>, ok: string, after?: (r: unknown) => void) => {
    try {
      const r = await fn();
      await refresh();
      toast.success(ok);
      after?.(r);
    } catch (e) {
      toast.error("Operação falhou", { description: errorMessage(e) });
    }
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-[1fr_160px] gap-3">
        <div>
          <Label htmlFor="sq-name">Nome</Label>
          <Input id="sq-name" value={form.name} onChange={(e) => set("name", e.target.value)} />
        </div>
        <div>
          <Label htmlFor="sq-mode">Modo padrão</Label>
          <select id="sq-mode" className={cn(select, "h-9 w-full")} value={form.defaultMode} onChange={(e) => set("defaultMode", e.target.value as MissionMode)}>
            {(Object.keys(MODE_INFO) as MissionMode[]).map((m) => (
              <option key={m} value={m}>
                {MODE_INFO[m].label}
              </option>
            ))}
          </select>
        </div>
      </div>
      <div>
        <Label htmlFor="sq-desc">Descrição</Label>
        <Input id="sq-desc" value={form.description} onChange={(e) => set("description", e.target.value)} />
      </div>
      <div>
        <Label>Membros (etapa = ordem no fluxo)</Label>
        <div className="space-y-1.5">
          {form.members.map((m, i) => (
            <div key={i} className="flex items-center gap-2">
              <select
                aria-label="Perfil"
                className={cn(select, "flex-1")}
                value={m.profileId}
                onChange={(e) => set("members", form.members.map((x, j) => (j === i ? { ...x, profileId: e.target.value } : x)))}
              >
                {profiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {PERMISSION_INFO[p.permission].short}
                  </option>
                ))}
              </select>
              <Input
                aria-label="Função"
                className="h-8 w-40"
                value={m.function}
                placeholder="Função"
                onChange={(e) => set("members", form.members.map((x, j) => (j === i ? { ...x, function: e.target.value } : x)))}
              />
              <select
                aria-label="Etapa"
                className={cn(select, "w-20")}
                value={m.stage}
                onChange={(e) => set("members", form.members.map((x, j) => (j === i ? { ...x, stage: Number(e.target.value) } : x)))}
              >
                {[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => (
                  <option key={n} value={n}>
                    etapa {n}
                  </option>
                ))}
              </select>
              <Button size="icon" variant="ghost" aria-label="Remover membro" onClick={() => set("members", form.members.filter((_, j) => j !== i))}>
                <Trash2 />
              </Button>
            </div>
          ))}
          <Button
            size="sm"
            variant="ghost"
            disabled={form.members.length >= 12}
            onClick={() => {
              const p = profiles.find((x) => !form.members.some((m) => m.profileId === x.id)) ?? profiles[0];
              if (p) set("members", [...form.members, { profileId: p.id, function: p.name, stage: Math.max(1, ...form.members.map((m) => m.stage)) }]);
            }}
          >
            <Plus /> Adicionar membro
          </Button>
        </div>
      </div>
      <div>
        <Label htmlFor="sq-flow">Fluxo</Label>
        <Input id="sq-flow" value={form.flow} placeholder="ex.: Architect → Backend + Frontend → QA" onChange={(e) => set("flow", e.target.value)} />
      </div>
      <div>
        <Label htmlFor="sq-rules">Regras (enviadas ao planejador e a cada tarefa)</Label>
        <Textarea id="sq-rules" rows={5} value={form.rules} onChange={(e) => set("rules", e.target.value)} className="text-[12px]" />
      </div>
      {squad && (
        <div>
          <Label>Skills do squad (aplicadas a todas as tarefas)</Label>
          <SkillChips kind="squad" targetId={squad.id} />
          <Label className="mt-3">Servidores MCP do squad (Claude Code)</Label>
          <McpChips kind="squad" targetId={squad.id} />
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {squad && !squad.builtin && (
          <Button variant="ghost" onClick={() => void act(() => api.squadDelete(squad.id), "Squad excluído", onDeleted)}>
            <Trash2 /> Excluir
          </Button>
        )}
        {squad?.builtin && (
          <Button variant="ghost" onClick={() => void act(() => api.squadRestore(squad.id), "Squad padrão restaurado")}>
            <RotateCcw /> Restaurar padrão
          </Button>
        )}
        {squad && (
          <>
            <Button variant="ghost" onClick={() => void act(() => api.squadCreate({ ...form, name: `${form.name} (cópia)` }), "Squad duplicado", (r) => onSaved((r as Squad).id))}>
              <Copy /> Duplicar
            </Button>
            <Button variant="ghost" onClick={() => void exportBundle("squad", squad.id)}>
              <Download /> Exportar
            </Button>
          </>
        )}
        <Button
          variant="primary"
          className="ml-auto"
          disabled={!dirty}
          onClick={() => void act(() => (squad ? api.squadUpdate(squad.id, form) : api.squadCreate(form)), "Squad salvo", (r) => onSaved((r as Squad).id))}
        >
          <Save /> Salvar
        </Button>
      </div>
    </div>
  );
}

// ── Recipes ───────────────────────────────────────────────────────────────

const toRecipeInput = (r: Recipe): RecipeInput => ({
  name: r.name,
  objective: r.objective,
  squadId: r.squadId,
  mode: r.mode,
  maxParallel: r.maxParallel,
  maxAttempts: r.maxAttempts,
  maxRuns: r.maxRuns,
  budgetUsd: r.budgetUsd,
  requireReview: r.requireReview,
  briefingTemplate: r.briefingTemplate,
  fields: r.fields,
  plan: r.plan,
});

function RecipeEditor({ recipe, onSaved, onDeleted }: { recipe: Recipe | null; onSaved: (id: string) => void; onDeleted: () => void }) {
  const squads = useLibrary((s) => s.squads);
  const refresh = useLibrary((s) => s.refresh);
  const profiles = useAgents((s) => s.profiles);
  const empty: RecipeInput = {
    name: "Nova receita",
    objective: "",
    squadId: null,
    mode: "squad",
    maxParallel: 2,
    maxAttempts: 3,
    maxRuns: 40,
    budgetUsd: null,
    requireReview: true,
    briefingTemplate: "Descreva aqui o briefing. Use {{campo}} para valores preenchidos ao usar a receita.",
    fields: [{ key: "campo", label: "Campo", placeholder: "", required: true }],
    plan: null,
  };
  const [form, setForm] = useState<RecipeInput>(recipe ? toRecipeInput(recipe) : empty);
  useEffect(() => setForm(recipe ? toRecipeInput(recipe) : empty), [recipe]); // eslint-disable-line react-hooks/exhaustive-deps
  const set = <K extends keyof RecipeInput>(k: K, v: RecipeInput[K]) => setForm((f) => ({ ...f, [k]: v }));
  const setField = (i: number, patch: Partial<RecipeField>) => set("fields", form.fields.map((f, j) => (j === i ? { ...f, ...patch } : f)));
  const dirty = recipe ? JSON.stringify(toRecipeInput(recipe)) !== JSON.stringify(form) : true;

  const act = async (fn: () => Promise<unknown>, ok: string, after?: (r: unknown) => void) => {
    try {
      const r = await fn();
      await refresh();
      toast.success(ok);
      after?.(r);
    } catch (e) {
      toast.error("Operação falhou", { description: errorMessage(e) });
    }
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3">
        <div>
          <Label htmlFor="rc-name">Nome</Label>
          <Input id="rc-name" value={form.name} onChange={(e) => set("name", e.target.value)} />
        </div>
        <div>
          <Label htmlFor="rc-squad">Squad</Label>
          <select id="rc-squad" className={cn(select, "h-9 w-full")} value={form.squadId ?? ""} onChange={(e) => set("squadId", e.target.value || null)}>
            <option value="">Sem squad (todos os agentes)</option>
            {squads.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
      </div>
      <div>
        <Label htmlFor="rc-obj">Objetivo</Label>
        <Input id="rc-obj" value={form.objective} onChange={(e) => set("objective", e.target.value)} />
      </div>
      <div className="grid grid-cols-5 gap-2 text-[12px]">
        <div>
          <Label>Modo</Label>
          <select className={cn(select, "w-full")} value={form.mode} onChange={(e) => set("mode", e.target.value as MissionMode)}>
            {(Object.keys(MODE_INFO) as MissionMode[]).map((m) => (
              <option key={m} value={m}>
                {MODE_INFO[m].label}
              </option>
            ))}
          </select>
        </div>
        <div>
          <Label>Paralelo</Label>
          <select className={cn(select, "w-full")} value={form.maxParallel} onChange={(e) => set("maxParallel", Number(e.target.value))}>
            {[1, 2, 3, 4].map((n) => (
              <option key={n}>{n}</option>
            ))}
          </select>
        </div>
        <div>
          <Label>Tentativas</Label>
          <select className={cn(select, "w-full")} value={form.maxAttempts} onChange={(e) => set("maxAttempts", Number(e.target.value))}>
            {[1, 2, 3, 4, 5].map((n) => (
              <option key={n}>{n}</option>
            ))}
          </select>
        </div>
        <div>
          <Label>Execuções</Label>
          <Input className="h-8" type="number" min={1} max={500} value={form.maxRuns} onChange={(e) => set("maxRuns", Number(e.target.value) || 1)} />
        </div>
        <div>
          <Label>Orçamento US$</Label>
          <Input className="h-8" value={form.budgetUsd ?? ""} placeholder="—" onChange={(e) => set("budgetUsd", e.target.value ? Number(e.target.value.replace(",", ".")) || null : null)} />
        </div>
      </div>
      {form.mode === "free" && (
        <label className="flex items-center gap-2 text-[12.5px] text-muted">
          <Switch label="Revisar cada tarefa" checked={form.requireReview} onCheckedChange={(v) => set("requireReview", v)} /> Revisar cada tarefa
        </label>
      )}
      <div>
        <Label>Campos</Label>
        <div className="space-y-1.5">
          {form.fields.map((f, i) => (
            <div key={i} className="flex items-center gap-2">
              <Input aria-label="Chave" className="h-8 w-28 font-mono text-[11.5px]" value={f.key} onChange={(e) => setField(i, { key: e.target.value.replace(/[^A-Za-z0-9_]/g, "") })} />
              <Input aria-label="Rótulo" className="h-8 flex-1" value={f.label} placeholder="Rótulo" onChange={(e) => setField(i, { label: e.target.value })} />
              <Input aria-label="Exemplo" className="h-8 flex-1" value={f.placeholder} placeholder="Exemplo" onChange={(e) => setField(i, { placeholder: e.target.value })} />
              <label className="flex items-center gap-1 text-[11.5px] text-muted">
                <input type="checkbox" className="accent-[#579bff]" checked={f.required} onChange={(e) => setField(i, { required: e.target.checked })} /> obrig.
              </label>
              <Button size="icon" variant="ghost" aria-label="Remover campo" onClick={() => set("fields", form.fields.filter((_, j) => j !== i))}>
                <Trash2 />
              </Button>
            </div>
          ))}
          <Button size="sm" variant="ghost" onClick={() => set("fields", [...form.fields, { key: `campo${form.fields.length + 1}`, label: "", placeholder: "", required: false }])}>
            <Plus /> Adicionar campo
          </Button>
        </div>
      </div>
      <div>
        <Label htmlFor="rc-tpl">Modelo de briefing (use {"{{chave}}"})</Label>
        <Textarea id="rc-tpl" rows={6} value={form.briefingTemplate} onChange={(e) => set("briefingTemplate", e.target.value)} className="text-[12px]" />
      </div>
      <div>
        <Label>Plano pronto</Label>
        {form.plan?.length ? (
          <div className="rounded-md border border-border bg-bg-2 p-2">
            <ol className="space-y-1 text-[12px] text-muted">
              {form.plan.map((t, i) => (
                <li key={t.key} className="flex gap-2">
                  <span className="text-subtle">{i + 1}.</span>
                  <span className="flex-1">{t.title}</span>
                  <span className="text-subtle">{profiles.find((p) => p.id === t.profileId)?.name ?? "—"}</span>
                </li>
              ))}
            </ol>
            <Button size="sm" variant="ghost" className="mt-1" onClick={() => set("plan", null)}>
              Remover plano (o squad planeja a cada missão)
            </Button>
          </div>
        ) : (
          <p className="text-[11.5px] text-subtle">Sem plano pronto: o planejador do squad cria o plano em cada missão. Para salvar um plano, use “Salvar como receita” numa missão.</p>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {recipe && !recipe.builtin && (
          <Button variant="ghost" onClick={() => void act(() => api.recipeDelete(recipe.id), "Receita excluída", onDeleted)}>
            <Trash2 /> Excluir
          </Button>
        )}
        {recipe?.builtin && (
          <Button variant="ghost" onClick={() => void act(() => api.recipeRestore(recipe.id), "Receita padrão restaurada")}>
            <RotateCcw /> Restaurar padrão
          </Button>
        )}
        {recipe && (
          <>
            <Button variant="ghost" onClick={() => void act(() => api.recipeCreate({ ...form, name: `${form.name} (cópia)` }), "Receita duplicada", (r) => onSaved((r as Recipe).id))}>
              <Copy /> Duplicar
            </Button>
            <Button variant="ghost" onClick={() => void exportBundle("recipe", recipe.id)}>
              <Download /> Exportar
            </Button>
          </>
        )}
        <Button
          variant="primary"
          className="ml-auto"
          disabled={!dirty}
          onClick={() => void act(() => (recipe ? api.recipeUpdate(recipe.id, form) : api.recipeCreate(form)), "Receita salva", (r) => onSaved((r as Recipe).id))}
        >
          <Save /> Salvar
        </Button>
      </div>
    </div>
  );
}

// ── Import ────────────────────────────────────────────────────────────────

function ImportDialog({ preview, onClose }: { preview: BundlePreview | null; onClose: () => void }) {
  const refresh = useLibrary((s) => s.refresh);
  const [busy, setBusy] = useState(false);
  if (!preview) return null;
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={`Importar ${preview.kind === "squad" ? "squad" : "receita"}: ${preview.name}`} description="Revise o conteúdo antes de importar. Nada foi adicionado ainda." className="w-[min(720px,calc(100vw-32px))]">
        <div className="max-h-[62vh] space-y-3 overflow-auto px-5 py-4 text-[12.5px]">
          {preview.warnings.map((w) => (
            <p key={w} className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-2 text-warning">
              <ShieldAlert className="mt-0.5 size-3.5 shrink-0" /> {w}
            </p>
          ))}
          {preview.squad && (
            <p>
              <span className="text-subtle">Squad:</span> {preview.squad}
            </p>
          )}
          {preview.recipe && (
            <p>
              <span className="text-subtle">Receita:</span> {preview.recipe}
            </p>
          )}
          {preview.profiles.length > 0 ? (
            <div>
              <div className="mb-1 text-[10.5px] uppercase tracking-wider text-subtle">Novos perfis de agente ({preview.profiles.length})</div>
              {preview.profiles.map((p) => (
                <div key={p.name} className="mb-2 rounded-md border border-border bg-bg-2 p-2.5">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-fg">{p.name}</span>
                    <Badge>{p.providerId}</Badge>
                    <Badge tone={p.permission === "read" ? "neutral" : "warning"}>{PERMISSION_INFO[p.permission].label}</Badge>
                  </div>
                  <pre className="selectable mt-1.5 max-h-40 overflow-auto font-mono text-[11px] whitespace-pre-wrap text-muted">{p.instructions || "(sem instruções)"}</pre>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-subtle">O bundle usa apenas perfis padrão do FORGE.</p>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            variant="primary"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                const r = await api.bundleImport(preview.bundle);
                await Promise.all([refresh(), useAgents.getState().refresh()]);
                toast.success("Importado", { description: `${r.profiles} perfil(is) novo(s)` });
                onClose();
              } catch (e) {
                toast.error("Importação recusada", { description: errorMessage(e) });
              } finally {
                setBusy(false);
              }
            }}
          >
            <Upload /> Importar
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ── View ──────────────────────────────────────────────────────────────────

export function LibraryView({ tab = "squads" }: { tab?: "squads" | "recipes" }) {
  const navigate = useApp((s) => s.navigate);
  const { squads, recipes, loaded, refresh } = useLibrary();
  const agentsLoaded = useAgents((s) => s.loaded);
  const [selected, setSelected] = useState<string | "new" | null>(null);
  const [using, setUsing] = useState<Recipe | null>(null);
  const [preview, setPreview] = useState<BundlePreview | null>(null);

  useEffect(() => {
    void refresh().catch((e) => toast.error("Falha ao carregar a biblioteca", { description: errorMessage(e) }));
    if (!agentsLoaded) void useAgents.getState().refresh().catch(() => undefined);
  }, [refresh, agentsLoaded]);
  useEffect(() => setSelected(null), [tab]);

  const items: (Squad | Recipe)[] = tab === "squads" ? squads : recipes;
  const current = selected === "new" ? null : (items.find((i) => i.id === selected) ?? null);

  const openImport = async () => {
    try {
      const p = await api.bundleOpen();
      if (p) setPreview(p);
    } catch (e) {
      toast.error("Arquivo recusado", { description: errorMessage(e) });
    }
  };

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-5 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Squads & Receitas</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Squads são equipes de agentes com funções, fluxo e regras. Receitas são modelos de missão (squad, modo do Maestro, limites, briefing com campos e, opcionalmente, um plano pronto).
            </p>
          </div>
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" onClick={() => void openImport()}>
              <Upload /> Importar…
            </Button>
            <Button variant="primary" onClick={() => setSelected("new")}>
              <Plus /> {tab === "squads" ? "Novo squad" : "Nova receita"}
            </Button>
          </div>
        </div>

        <div className="mb-4 flex gap-1 border-b border-border">
          {(["squads", "recipes"] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => navigate({ view: "library", tab: t })}
              className={cn("relative px-3 py-2 text-[13px]", tab === t ? "text-fg" : "text-subtle hover:text-muted")}
            >
              {t === "squads" ? `Squads (${squads.length})` : `Receitas (${recipes.length})`}
              {tab === t && <span className="absolute inset-x-2 bottom-0 h-px bg-accent" />}
            </button>
          ))}
        </div>

        {!loaded ? null : (
          <div className="grid gap-4 lg:grid-cols-[300px_1fr]">
            <div className="space-y-2">
              {items.map((it) => {
                const isRecipe = "briefingTemplate" in it;
                return (
                  <Card key={it.id} className={cn("cursor-default p-3 transition-colors", selected === it.id ? "border-accent/50" : "hover:border-border-strong")} onClick={() => setSelected(it.id)}>
                    <div className="flex items-center gap-2">
                      {isRecipe ? <BookOpen className="size-3.5 text-accent-2" /> : <Users className="size-3.5 text-accent" />}
                      <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-fg">{it.name}</span>
                      <OriginBadge origin={it.origin} />
                    </div>
                    <p className="mt-1 line-clamp-2 text-[11.5px] text-muted">{isRecipe ? (it as Recipe).objective : (it as Squad).description}</p>
                    {isRecipe ? (
                      <div className="mt-2 flex items-center gap-1.5">
                        <Badge>{MODE_INFO[(it as Recipe).mode].label}</Badge>
                        {(it as Recipe).plan && <Badge tone="success">plano</Badge>}
                        <Button
                          size="sm"
                          variant="secondary"
                          className="ml-auto h-6"
                          onClick={(e) => {
                            e.stopPropagation();
                            setUsing(it as Recipe);
                          }}
                        >
                          <Play /> Usar
                        </Button>
                      </div>
                    ) : (
                      <p className="mt-1.5 truncate text-[11px] text-subtle">{(it as Squad).flow || `${(it as Squad).members.length} membros`}</p>
                    )}
                  </Card>
                );
              })}
            </div>
            <Card className="h-fit p-5">
              {selected === null ? (
                <EmptyState icon={tab === "squads" ? <Users /> : <BookOpen />} title={tab === "squads" ? "Selecione um squad" : "Selecione uma receita"} />
              ) : tab === "squads" ? (
                <SquadEditor key={selected} squad={current as Squad | null} onSaved={(id) => setSelected(id)} onDeleted={() => setSelected(null)} />
              ) : (
                <RecipeEditor key={selected} recipe={current as Recipe | null} onSaved={(id) => setSelected(id)} onDeleted={() => setSelected(null)} />
              )}
            </Card>
          </div>
        )}
      </div>
      <UseRecipeDialog recipe={using} onClose={() => setUsing(null)} />
      <ImportDialog preview={preview} onClose={() => setPreview(null)} />
    </div>
  );
}
