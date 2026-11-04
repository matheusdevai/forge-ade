import { useCallback, useEffect, useMemo, useState } from "react";
import { Archive, ArchiveRestore, Brain, Check, Eye, Globe, Pencil, Pin, PinOff, Plus, Search, Sparkles, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, Dialog, DialogContent, EmptyState, Input, Label, Switch, Textarea } from "@/components/ui";
import { api, errorMessage, type Memory, type MemoryCounts, type MemoryInput, type MemoryKind, type MemoryPreview, type MemoryScope, type MemoryStatus } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useAgents } from "@/stores/agents";
import { useApp } from "@/stores/app";
import { useMissions, useWorkspaces } from "@/stores/data";
import { KIND_INFO, KINDS, SCOPE_LABEL, parseTags, scopeNeedsCrossProjectConsent } from "./model";

const select = "h-8 rounded-md border border-border bg-bg px-2 text-[12.5px] text-fg outline-none focus:border-accent/60";

function useLookups() {
  const workspaces = useWorkspaces((s) => s.items);
  const missions = useMissions((s) => s.items);
  const profiles = useAgents((s) => s.profiles);
  useEffect(() => {
    if (!useWorkspaces.getState().loaded) void useWorkspaces.getState().refresh().catch(() => undefined);
    if (!useMissions.getState().loaded) void useMissions.getState().refresh().catch(() => undefined);
    if (!useAgents.getState().loaded) void useAgents.getState().refresh().catch(() => undefined);
  }, []);
  return { workspaces, missions, profiles };
}

function ScopeBadge({ m }: { m: Memory }) {
  const { workspaces, missions, profiles } = useLookups();
  const ws = workspaces.find((w) => w.id === m.workspaceId)?.name;
  let text: string;
  if (m.scope === "global") text = "Global";
  else if (m.scope === "workspace") text = ws ?? "Workspace";
  else if (m.scope === "mission") text = `Missão · ${missions.find((x) => x.id === m.missionId)?.title ?? "—"}`;
  else text = `Agente · ${profiles.find((p) => p.id === m.profileId)?.name ?? "—"} · ${ws ?? "todos os projetos"}`;
  return (
    <Badge tone={m.scope === "global" || (m.scope === "agent" && !m.workspaceId) ? "warning" : "neutral"} title={SCOPE_LABEL[m.scope]}>
      {m.scope === "global" && <Globe className="size-3" />}
      {text}
    </Badge>
  );
}

function MemoryCard({ m, onEdit, onChanged }: { m: Memory; onEdit: () => void; onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const act = (p: Promise<unknown>, ok?: string) =>
    void p
      .then(() => {
        if (ok) toast.success(ok);
        onChanged();
      })
      .catch((e) => toast.error("Falha", { description: errorMessage(e) }));
  const long = m.body.length > 280 || m.body.split("\n").length > 4;
  return (
    <Card className={cn("px-4 py-3", m.status === "suggested" && "border-accent/40", m.status === "archived" && "opacity-70")}>
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge tone={KIND_INFO[m.kind].tone}>{KIND_INFO[m.kind].label}</Badge>
        <ScopeBadge m={m} />
        {m.pinned && (
          <Badge tone="accent">
            <Pin className="size-3" /> fixada
          </Badge>
        )}
        {m.source === "agent" && (
          <Badge tone="accent" title="Proposta por um agente">
            <Sparkles className="size-3" /> sugerida por agente
          </Badge>
        )}
        <span className="ml-auto text-[11px] text-subtle">{relativeTime(m.updatedAt)}</span>
      </div>
      <h3 className="mt-2 text-[13.5px] font-medium text-fg">{m.title}</h3>
      <p className={cn("mt-1 text-[12.5px] leading-relaxed whitespace-pre-wrap text-muted", !open && long && "line-clamp-4")}>{m.body}</p>
      {long && (
        <button className="mt-1 text-[11.5px] text-accent hover:underline" onClick={() => setOpen((v) => !v)}>
          {open ? "Mostrar menos" : "Mostrar tudo"}
        </button>
      )}
      {m.tags.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {m.tags.map((t) => (
            <span key={t} className="rounded bg-elevated px-1.5 py-px font-mono text-[10.5px] text-subtle">
              #{t}
            </span>
          ))}
        </div>
      )}
      <div className="mt-3 flex flex-wrap gap-1">
        {m.status === "suggested" ? (
          <>
            <Button size="sm" variant="primary" onClick={() => act(api.memoryApprove(m.id), "Memória aprovada — agentes passam a recebê-la")}>
              <Check /> Aprovar
            </Button>
            <Button size="sm" variant="ghost" onClick={onEdit}>
              <Pencil /> Editar antes
            </Button>
            <Button size="sm" variant="ghost" onClick={() => act(api.memoryDelete(m.id), "Sugestão rejeitada")}>
              <X /> Rejeitar
            </Button>
          </>
        ) : (
          <>
            <Button size="sm" variant="ghost" onClick={onEdit}>
              <Pencil /> Editar
            </Button>
            {m.status === "active" && (
              <Button size="sm" variant="ghost" onClick={() => act(api.memorySetPinned(m.id, !m.pinned))}>
                {m.pinned ? <PinOff /> : <Pin />} {m.pinned ? "Desafixar" : "Fixar"}
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => act(api.memorySetStatus(m.id, m.status === "archived" ? "active" : "archived"))}>
              {m.status === "archived" ? <ArchiveRestore /> : <Archive />} {m.status === "archived" ? "Reativar" : "Arquivar"}
            </Button>
            <Button size="sm" variant="ghost" className="ml-auto text-error hover:text-error" onClick={() => setConfirmDelete(true)}>
              <Trash2 /> Excluir
            </Button>
          </>
        )}
      </div>
      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent title="Excluir memória?" description={`“${m.title}” será apagada definitivamente. Para apenas parar de enviá-la aos agentes, use Arquivar.`}>
          <div className="flex justify-end gap-2 px-5 py-4">
            <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
              Cancelar
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setConfirmDelete(false);
                act(api.memoryDelete(m.id), "Memória excluída");
              }}
            >
              <Trash2 /> Excluir
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

function blankInput(workspaceId: string | null): MemoryInput {
  return {
    scope: workspaceId ? "workspace" : "global",
    workspaceId,
    missionId: null,
    profileId: null,
    kind: "convention",
    title: "",
    body: "",
    tags: [],
    pinned: false,
  };
}

function Editor({ initial, editing, onClose, onSaved }: { initial: MemoryInput; editing: Memory | null; onClose: () => void; onSaved: () => void }) {
  const { workspaces, missions, profiles } = useLookups();
  const [v, setV] = useState<MemoryInput>(initial);
  const [tags, setTags] = useState(initial.tags.join(", "));
  const [consent, setConsent] = useState(false);
  const [saving, setSaving] = useState(false);
  const set = (patch: Partial<MemoryInput>) => setV((cur) => ({ ...cur, ...patch }));
  const wasCross = editing ? scopeNeedsCrossProjectConsent(editing) : false;
  const needsConsent = scopeNeedsCrossProjectConsent(v) && !wasCross;

  const save = async () => {
    setSaving(true);
    try {
      const input = { ...v, tags: parseTags(tags) };
      if (editing) await api.memoryUpdate(editing.id, input);
      else await api.memoryCreate(input);
      toast.success(editing ? "Memória atualizada" : "Memória criada");
      onSaved();
    } catch (e) {
      toast.error("Não foi possível salvar", { description: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={editing ? "Editar memória" : "Nova memória"} description="Informação durável que os agentes recebem como contexto. Nunca guarde segredos aqui." className="w-[min(680px,calc(100vw-32px))]">
        <div className="max-h-[65vh] space-y-3 overflow-auto px-5 py-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label>Tipo</Label>
              <select className={cn(select, "w-full")} value={v.kind} onChange={(e) => set({ kind: e.target.value as MemoryKind })}>
                {KINDS.map((k) => (
                  <option key={k} value={k}>
                    {KIND_INFO[k].label}
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-1">
              <Label>Escopo</Label>
              <select className={cn(select, "w-full")} value={v.scope} onChange={(e) => set({ scope: e.target.value as MemoryScope })}>
                {(["workspace", "mission", "agent", "global"] as MemoryScope[]).map((s) => (
                  <option key={s} value={s}>
                    {SCOPE_LABEL[s]}
                  </option>
                ))}
              </select>
            </div>
          </div>
          {v.scope === "workspace" && (
            <div className="space-y-1">
              <Label>Workspace</Label>
              <select className={cn(select, "w-full")} value={v.workspaceId ?? ""} onChange={(e) => set({ workspaceId: e.target.value || null })}>
                <option value="">Escolha…</option>
                {workspaces.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </div>
          )}
          {v.scope === "mission" && (
            <div className="space-y-1">
              <Label>Missão</Label>
              <select className={cn(select, "w-full")} value={v.missionId ?? ""} onChange={(e) => set({ missionId: e.target.value || null })}>
                <option value="">Escolha…</option>
                {missions
                  .filter((m) => m.workspaceId)
                  .map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.title} · {workspaces.find((w) => w.id === m.workspaceId)?.name ?? "—"}
                    </option>
                  ))}
              </select>
            </div>
          )}
          {v.scope === "agent" && (
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1">
                <Label>Agente</Label>
                <select className={cn(select, "w-full")} value={v.profileId ?? ""} onChange={(e) => set({ profileId: e.target.value || null })}>
                  <option value="">Escolha…</option>
                  {profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-1">
                <Label>Vale em</Label>
                <select className={cn(select, "w-full")} value={v.workspaceId ?? ""} onChange={(e) => set({ workspaceId: e.target.value || null })}>
                  <option value="">Todos os projetos</option>
                  {workspaces.map((w) => (
                    <option key={w.id} value={w.id}>
                      {w.name}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          )}
          {needsConsent && (
            <label className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/8 px-3 py-2 text-[12px] text-warning">
              <input type="checkbox" className="mt-0.5" checked={consent} onChange={(e) => setConsent(e.target.checked)} />
              <span>Autorizo compartilhar esta memória entre projetos: agentes de qualquer workspace passarão a recebê-la.</span>
            </label>
          )}
          <div className="space-y-1">
            <Label>Título</Label>
            <Input value={v.title} onChange={(e) => set({ title: e.target.value })} placeholder="Ex.: Use pnpm, nunca npm" maxLength={160} />
          </div>
          <div className="space-y-1">
            <Label>Conteúdo</Label>
            <Textarea value={v.body} onChange={(e) => set({ body: e.target.value })} rows={8} placeholder="O que os agentes precisam saber, e por quê." />
          </div>
          <div className="grid gap-3 sm:grid-cols-[1fr_auto] sm:items-end">
            <div className="space-y-1">
              <Label>Tags (separadas por vírgula)</Label>
              <Input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="api, banco, deploy" />
            </div>
            <label className="flex h-8 items-center gap-2 text-[12.5px] text-muted">
              <Switch checked={v.pinned} onCheckedChange={(c) => set({ pinned: c })} label="Fixar" /> Fixar (prioridade)
            </label>
          </div>
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="primary" disabled={saving || !v.title.trim() || !v.body.trim() || (needsConsent && !consent)} onClick={() => void save()}>
            Salvar
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function PreviewCard({ workspaceId }: { workspaceId: string | null }) {
  const { workspaces, profiles } = useLookups();
  const [ws, setWs] = useState<string>(workspaceId ?? "");
  const [profile, setProfile] = useState<string>("");
  const [preview, setPreview] = useState<MemoryPreview | null>(null);
  useEffect(() => setWs(workspaceId ?? ""), [workspaceId]);
  const load = useCallback(() => {
    api
      .memoryPreview(ws || null, null, profile || null)
      .then(setPreview)
      .catch(() => setPreview(null));
  }, [ws, profile]);
  useEffect(load, [load]);
  return (
    <Card className="px-4 py-3">
      <div className="flex items-center gap-2 text-[13px] font-medium">
        <Eye className="size-4 text-accent" /> O que um agente recebe
      </div>
      <p className="mt-1 text-[11.5px] leading-relaxed text-muted">Exatamente o que o FORGE injeta numa execução nesse contexto (missões somam as memórias da missão).</p>
      <div className="mt-2 space-y-2">
        <select className={cn(select, "w-full")} value={ws} onChange={(e) => setWs(e.target.value)}>
          <option value="">Sem workspace</option>
          {workspaces.map((w) => (
            <option key={w.id} value={w.id}>
              {w.name}
            </option>
          ))}
        </select>
        <select className={cn(select, "w-full")} value={profile} onChange={(e) => setProfile(e.target.value)}>
          <option value="">Qualquer agente</option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>
      {preview && (
        <div className="mt-3 text-[12px]">
          {preview.labels.length === 0 ? (
            <p className="text-subtle">Nenhuma memória ativa se aplica.</p>
          ) : (
            <ul className="space-y-1">
              {preview.labels.map((l, i) => (
                <li key={i} className="truncate text-muted" title={l}>
                  · {l}
                </li>
              ))}
            </ul>
          )}
          <p className="mt-2 text-[11px] text-subtle">{(preview.bytes / 1024).toFixed(1)} KB de instruções (limite 24 KB)</p>
          {preview.dropped.length > 0 && <p className="mt-1 text-[11px] text-warning">Fora do limite: {preview.dropped.join(", ")}</p>}
        </div>
      )}
    </Card>
  );
}

export function MemoryView({ workspaceId }: { workspaceId?: string }) {
  const navigate = useApp((s) => s.navigate);
  const { workspaces } = useLookups();
  const [status, setStatus] = useState<MemoryStatus>("active");
  const [scope, setScope] = useState<string>(workspaceId ? `ws:${workspaceId}` : "");
  const [kind, setKind] = useState<MemoryKind | "">("");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<Memory[] | null>(null);
  const [counts, setCounts] = useState<MemoryCounts | null>(null);
  const [editor, setEditor] = useState<{ initial: MemoryInput; editing: Memory | null } | null>(null);

  useEffect(() => setScope(workspaceId ? `ws:${workspaceId}` : ""), [workspaceId]);
  const wsFilter = scope.startsWith("ws:") ? scope.slice(3) : null;

  const load = useCallback(() => {
    const scopeFilter = scope === "global" || scope === "agent" || scope === "mission" ? (scope as MemoryScope) : null;
    api
      .memoryList({ status, workspaceId: wsFilter, scope: scopeFilter, kind: kind || null, query: query.trim() || null })
      .then(setItems)
      .catch((e) => toast.error("Falha ao carregar memória", { description: errorMessage(e) }));
    api.memoryCounts(wsFilter).then(setCounts).catch(() => undefined);
  }, [status, scope, wsFilter, kind, query]);

  useEffect(() => {
    const t = setTimeout(load, query ? 200 : 0);
    return () => clearTimeout(t);
  }, [load, query]);

  const tabs: { id: MemoryStatus; label: string; n?: number }[] = useMemo(
    () => [
      { id: "active", label: "Ativas", n: counts?.active },
      { id: "suggested", label: "Sugestões", n: counts?.suggested },
      { id: "archived", label: "Arquivadas", n: counts?.archived },
    ],
    [counts],
  );

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-5 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Memória</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Decisões, convenções, contexto e erros recorrentes que os agentes recebem automaticamente. Memórias de um workspace nunca vão para outro projeto; compartilhar entre
              projetos exige sua autorização. Sugestões dos agentes só valem depois de aprovadas.
            </p>
          </div>
          <div className="ml-auto">
            <Button variant="primary" onClick={() => setEditor({ initial: blankInput(wsFilter), editing: null })}>
              <Plus /> Nova memória
            </Button>
          </div>
        </div>

        <div className="mb-4 flex flex-wrap items-center gap-2">
          <div className="flex rounded-md border border-border bg-bg p-0.5">
            {tabs.map((t) => (
              <button
                key={t.id}
                onClick={() => setStatus(t.id)}
                className={cn("flex items-center gap-1.5 rounded px-2.5 py-1 text-[12px]", status === t.id ? "bg-elevated text-fg" : "text-muted hover:text-fg")}
              >
                {t.label}
                {t.n ? <span className={cn("text-[10.5px]", t.id === "suggested" ? "text-accent" : "text-subtle")}>{t.n}</span> : null}
              </button>
            ))}
          </div>
          <select
            className={select}
            value={scope}
            onChange={(e) => {
              const v = e.target.value;
              setScope(v);
              if (v.startsWith("ws:")) navigate({ view: "memory", workspaceId: v.slice(3) });
              else if (workspaceId) navigate({ view: "memory" });
            }}
            aria-label="Escopo"
          >
            <option value="">Todos os escopos</option>
            <option value="global">Global</option>
            <option value="mission">Missões</option>
            <option value="agent">Agentes</option>
            {workspaces.map((w) => (
              <option key={w.id} value={`ws:${w.id}`}>
                Workspace · {w.name}
              </option>
            ))}
          </select>
          <select className={select} value={kind} onChange={(e) => setKind(e.target.value as MemoryKind | "")} aria-label="Tipo">
            <option value="">Todos os tipos</option>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {KIND_INFO[k].label}
              </option>
            ))}
          </select>
          <div className="relative min-w-[200px] flex-1">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-subtle" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Buscar em títulos, conteúdo e tags" className="pl-8" />
          </div>
        </div>

        <div className="grid gap-4 lg:grid-cols-[1fr_280px]">
          <div className="space-y-3">
            {!items ? null : items.length === 0 ? (
              <Card>
                <EmptyState
                  icon={status === "suggested" ? <Sparkles /> : <Brain />}
                  title={status === "suggested" ? "Nenhuma sugestão pendente" : status === "archived" ? "Nada arquivado" : query || kind || scope ? "Nada encontrado" : "Nenhuma memória ainda"}
                  description={
                    status === "suggested"
                      ? "Quando um agente concluir uma execução e propuser algo durável (<forge-memory>), a proposta aparece aqui para você aprovar, editar ou rejeitar."
                      : status === "active" && !query && !kind
                        ? "Registre decisões arquiteturais, convenções do projeto, erros recorrentes e preferências. Os agentes desse escopo passam a recebê-las."
                        : undefined
                  }
                  action={
                    status === "active" && !query && !kind ? (
                      <Button variant="primary" onClick={() => setEditor({ initial: blankInput(wsFilter), editing: null })}>
                        <Plus /> Nova memória
                      </Button>
                    ) : undefined
                  }
                />
              </Card>
            ) : (
              items.map((m) => (
                <MemoryCard
                  key={m.id}
                  m={m}
                  onChanged={load}
                  onEdit={() =>
                    setEditor({
                      editing: m,
                      initial: { scope: m.scope, workspaceId: m.workspaceId, missionId: m.missionId, profileId: m.profileId, kind: m.kind, title: m.title, body: m.body, tags: m.tags, pinned: m.pinned },
                    })
                  }
                />
              ))
            )}
          </div>
          <div className="space-y-3">
            <PreviewCard workspaceId={wsFilter} />
          </div>
        </div>
      </div>
      {editor && (
        <Editor
          initial={editor.initial}
          editing={editor.editing}
          onClose={() => setEditor(null)}
          onSaved={() => {
            setEditor(null);
            load();
          }}
        />
      )}
    </div>
  );
}
