import { useCallback, useEffect, useState } from "react";
import { Download, FileCode2, FilePlus2, FolderInput, History, Plus, RotateCcw, Save, ShieldAlert, Sparkle, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, CardHeader, Dialog, DialogContent, EmptyState, Input, Label, Switch, Textarea } from "@/components/ui";
import { api, errorMessage, type Skill, type SkillDetail, type SkillDoc, type SkillPreview } from "@/lib/ipc";
import { cn, formatBytes, relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useAgents } from "@/stores/agents";
import { useLibrary } from "@/stores/library";

function Associations({ detail, onChange }: { detail: SkillDetail; onChange: () => void }) {
  const profiles = useAgents((s) => s.profiles);
  const squads = useLibrary((s) => s.squads);
  const linked = (kind: string, id: string) => detail.links.some((l) => l.targetKind === kind && l.targetId === id);
  const toggle = async (kind: "profile" | "squad", id: string, on: boolean) => {
    try {
      await api.skillSetLink(detail.skill.id, kind, id, on);
      onChange();
    } catch (e) {
      toast.error("Falha ao associar", { description: errorMessage(e) });
    }
  };
  const missions = detail.links.filter((l) => l.targetKind === "mission").length;
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <div>
        <div className="mb-1.5 text-[10.5px] uppercase tracking-wider text-subtle">Agentes</div>
        <div className="max-h-56 space-y-1 overflow-auto">
          {profiles.map((p) => (
            <label key={p.id} className="flex items-center gap-2 text-[12.5px] text-muted">
              <input type="checkbox" className="accent-[#579bff]" checked={linked("profile", p.id)} onChange={(e) => void toggle("profile", p.id, e.target.checked)} />
              {p.name}
            </label>
          ))}
        </div>
      </div>
      <div>
        <div className="mb-1.5 text-[10.5px] uppercase tracking-wider text-subtle">Squads</div>
        <div className="max-h-56 space-y-1 overflow-auto">
          {squads.map((s) => (
            <label key={s.id} className="flex items-center gap-2 text-[12.5px] text-muted">
              <input type="checkbox" className="accent-[#579bff]" checked={linked("squad", s.id)} onChange={(e) => void toggle("squad", s.id, e.target.checked)} />
              {s.name}
            </label>
          ))}
        </div>
        <p className="mt-2 text-[11px] text-subtle">
          {missions} missão(ões) usam esta skill. Associe a missões pela tela da missão.
        </p>
      </div>
    </div>
  );
}

function SkillEditor({ id, onChanged, onDeleted }: { id: string; onChanged: () => void; onDeleted: () => void }) {
  const [detail, setDetail] = useState<SkillDetail | null>(null);
  const [form, setForm] = useState<SkillDoc>({ name: "", description: "", body: "" });
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [viewVersion, setViewVersion] = useState<number | null>(null);

  const load = useCallback(() => {
    api
      .skillDetail(id)
      .then((d) => {
        setDetail(d);
        setForm({ name: d.skill.name, description: d.skill.description, body: d.skill.body });
      })
      .catch((e) => toast.error("Falha ao carregar a skill", { description: errorMessage(e) }));
  }, [id]);
  useEffect(load, [load]);

  if (!detail) return null;
  const s = detail.skill;
  const dirty = form.name !== s.name || form.description !== s.description || form.body !== s.body;
  const act = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn();
      toast.success(ok);
      load();
      onChanged();
    } catch (e) {
      toast.error("Operação falhou", { description: errorMessage(e) });
    }
  };
  const shown = viewVersion !== null ? detail.versions.find((v) => v.version === viewVersion) : null;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-[15px] font-semibold">{s.name}</h2>
        <Badge>v{s.version}</Badge>
        <Badge>{s.slug}</Badge>
        {s.origin === "import" && <Badge tone="warning">importada</Badge>}
        {s.builtin && <Badge tone="accent">padrão</Badge>}
        <label className="ml-auto flex items-center gap-2 text-[12px] text-muted">
          <Switch label="Ativa" checked={s.enabled} onCheckedChange={(v) => void act(() => api.skillSetEnabled(s.id, v), v ? "Skill ativada" : "Skill desativada")} />
          {s.enabled ? "ativa" : "desativada"}
        </label>
      </div>
      {s.source && <p className="truncate font-mono text-[11px] text-subtle">origem: {s.source}</p>}

      <div className="grid grid-cols-[1fr_2fr] gap-3">
        <div>
          <Label htmlFor="sk-name">Nome</Label>
          <Input id="sk-name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
        </div>
        <div>
          <Label htmlFor="sk-desc">Descrição (quando usar)</Label>
          <Input id="sk-desc" value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} />
        </div>
      </div>
      <div>
        <Label htmlFor="sk-body">Instruções (corpo do SKILL.md, Markdown)</Label>
        <Textarea id="sk-body" rows={14} value={form.body} onChange={(e) => setForm({ ...form, body: e.target.value })} className="font-mono text-[12px]" />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {!s.builtin && (
          <Button variant="ghost" onClick={() => setConfirmDelete(true)}>
            <Trash2 /> Remover
          </Button>
        )}
        {s.builtin && (
          <Button variant="ghost" onClick={() => void act(() => api.skillRestoreBuiltin(s.id), "Conteúdo padrão restaurado (nova versão)")}>
            <RotateCcw /> Restaurar padrão
          </Button>
        )}
        <Button
          variant="ghost"
          onClick={() =>
            void api
              .skillExport(s.id)
              .then((p) => p && toast.success("Skill exportada", { description: p }))
              .catch((e) => toast.error("Falha ao exportar", { description: errorMessage(e) }))
          }
        >
          <Download /> Exportar pasta
        </Button>
        <Button variant="primary" className="ml-auto" disabled={!dirty} onClick={() => void act(() => api.skillUpdate(s.id, form), `Salva como v${s.version + 1}`)}>
          <Save /> Salvar nova versão
        </Button>
      </div>

      <Card>
        <CardHeader title="Associações" />
        <div className="p-4">
          <Associations detail={detail} onChange={load} />
          <p className="mt-3 text-[11px] leading-relaxed text-subtle">
            Skills ativas associadas ao agente, ao squad ou à missão são adicionadas às instruções de cada execução (em qualquer provider) e registradas na execução. Nada é gravado no seu repositório.
          </p>
        </div>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader title="Versões" icon={<History />} />
          <ul className="max-h-64 overflow-auto p-2">
            {detail.versions.map((v) => (
              <li key={v.version} className="flex items-center gap-2 rounded px-2 py-1.5 text-[12px] hover:bg-elevated">
                <span className="font-mono text-subtle">v{v.version}</span>
                <span className="min-w-0 flex-1 truncate text-muted">{v.note || "—"}</span>
                <span className="text-[11px] text-subtle">{relativeTime(v.createdAt)}</span>
                <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => setViewVersion(v.version)}>
                  ver
                </Button>
                {v.version !== s.version && (
                  <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => void act(() => api.skillRestoreVersion(s.id, v.version), `v${v.version} restaurada como nova versão`)}>
                    restaurar
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </Card>
        <Card>
          <CardHeader title="Arquivos de apoio" icon={<FileCode2 />} />
          {detail.files.length === 0 ? (
            <p className="p-4 text-[12px] text-subtle">Nenhum arquivo além do SKILL.md.</p>
          ) : (
            <ul className="max-h-64 overflow-auto p-2">
              {detail.files.map((f) => (
                <li key={f.path} className="flex items-center gap-2 px-2 py-1 text-[12px]">
                  <span className="min-w-0 flex-1 truncate font-mono text-muted">{f.path}</span>
                  {f.isScript && <Badge tone="warning">script · nunca executado</Badge>}
                  <span className="text-[11px] text-subtle">{formatBytes(f.size)}</span>
                </li>
              ))}
            </ul>
          )}
          <p className="border-t border-border px-4 py-2 text-[11px] text-subtle">Arquivos de apoio são guardados para referência; não são executados nem enviados aos agentes.</p>
        </Card>
      </div>

      <Dialog open={!!shown} onOpenChange={(o) => !o && setViewVersion(null)}>
        {shown && (
          <DialogContent title={`${shown.name} — v${shown.version}`} description={shown.description} className="w-[min(760px,calc(100vw-32px))]">
            <pre className="selectable max-h-[60vh] overflow-auto px-5 py-4 font-mono text-[12px] whitespace-pre-wrap text-muted">{shown.body}</pre>
          </DialogContent>
        )}
      </Dialog>
      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent title="Remover skill?" description={s.name}>
          <div className="px-5 py-4 text-[13px] text-muted">
            A skill, suas versões e associações serão removidas. Execuções passadas mantêm o registro de que a usaram.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirmDelete(false);
                  void api
                    .skillDelete(s.id)
                    .then(() => {
                      toast.success("Skill removida");
                      onDeleted();
                    })
                    .catch((e) => toast.error("Falha", { description: errorMessage(e) }));
                }}
              >
                Remover
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function InstallDialog({ preview, onClose, onInstalled }: { preview: SkillPreview | null; onClose: () => void; onInstalled: (s: Skill) => void }) {
  const [update, setUpdate] = useState(false);
  useEffect(() => setUpdate(false), [preview]);
  if (!preview) return null;
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={`Instalar skill: ${preview.doc.name}`} description={preview.doc.description} className="w-[min(760px,calc(100vw-32px))]">
        <div className="max-h-[62vh] space-y-3 overflow-auto px-5 py-4 text-[12.5px]">
          <p className="truncate font-mono text-[11px] text-subtle">{preview.source}</p>
          {preview.warnings.map((w) => (
            <p key={w} className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-2 text-warning">
              <ShieldAlert className="mt-0.5 size-3.5 shrink-0" /> {w}
            </p>
          ))}
          <div>
            <div className="mb-1 text-[10.5px] uppercase tracking-wider text-subtle">Instruções que serão enviadas aos agentes</div>
            <pre className="selectable max-h-72 overflow-auto rounded-md border border-border bg-bg-2 p-3 font-mono text-[11.5px] whitespace-pre-wrap text-muted">{preview.doc.body}</pre>
          </div>
          {preview.files.length > 0 && (
            <div>
              <div className="mb-1 text-[10.5px] uppercase tracking-wider text-subtle">Arquivos de apoio ({preview.files.length})</div>
              {preview.files.map((f) => (
                <div key={f.path} className="flex items-center gap-2 text-[12px]">
                  <span className="flex-1 truncate font-mono text-muted">{f.path}</span>
                  {f.isScript && <Badge tone="warning">script · nunca executado</Badge>}
                  <span className="text-subtle">{formatBytes(f.size)}</span>
                </div>
              ))}
            </div>
          )}
          {preview.exists && (
            <label className="flex items-center gap-2 text-warning">
              <input type="checkbox" className="accent-[#e9ae52]" checked={update} onChange={(e) => setUpdate(e.target.checked)} />
              Já existe uma skill com este nome — atualizar como nova versão
            </label>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            variant="primary"
            disabled={preview.exists && !update}
            onClick={() =>
              void api
                .skillInstall(preview.token, update)
                .then((s) => {
                  toast.success(`Skill instalada (v${s.version})`);
                  onInstalled(s);
                })
                .catch((e) => toast.error("Instalação recusada", { description: errorMessage(e) }))
            }
          >
            Instalar
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function SkillsView({ id }: { id?: string }) {
  const navigate = useApp((s) => s.navigate);
  const [skills, setSkills] = useState<Skill[] | null>(null);
  const [preview, setPreview] = useState<SkillPreview | null>(null);
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState<SkillDoc>({ name: "", description: "", body: "" });
  const agentsLoaded = useAgents((s) => s.loaded);
  const libLoaded = useLibrary((s) => s.loaded);

  const load = useCallback(() => {
    api.skillsList().then(setSkills).catch((e) => toast.error("Falha ao carregar skills", { description: errorMessage(e) }));
  }, []);
  useEffect(() => {
    load();
    if (!agentsLoaded) void useAgents.getState().refresh().catch(() => undefined);
    if (!libLoaded) void useLibrary.getState().refresh().catch(() => undefined);
  }, [load, agentsLoaded, libLoaded]);
  useEffect(() => {
    if (!id && skills?.[0]) navigate({ view: "skills", id: skills[0].id });
  }, [id, skills, navigate]);

  const pick = async (folder: boolean) => {
    try {
      const p = await api.skillPick(folder);
      if (p) setPreview(p);
    } catch (e) {
      toast.error("Skill recusada", { description: errorMessage(e) });
    }
  };

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-5 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Skills</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Instruções estruturadas e versionadas no formato <code className="font-mono text-fg">SKILL.md</code> (compatível com o ecossistema de skills de agentes). Associe a agentes, squads ou missões.
            </p>
          </div>
          <div className="ml-auto flex flex-wrap gap-2">
            <Button variant="ghost" onClick={() => void pick(true)}>
              <FolderInput /> Instalar pasta…
            </Button>
            <Button variant="ghost" onClick={() => void pick(false)}>
              <FilePlus2 /> Instalar SKILL.md…
            </Button>
            <Button variant="primary" onClick={() => setCreating(true)}>
              <Plus /> Nova skill
            </Button>
          </div>
        </div>
        {!skills ? null : skills.length === 0 ? (
          <EmptyState icon={<Sparkle />} title="Nenhuma skill" />
        ) : (
          <div className="grid gap-4 lg:grid-cols-[280px_1fr]">
            <Card className="h-fit overflow-hidden">
              <ul className="p-1.5">
                {skills.map((s) => (
                  <li key={s.id}>
                    <button
                      type="button"
                      onClick={() => navigate({ view: "skills", id: s.id })}
                      className={cn("flex w-full items-start gap-2 rounded-md px-2.5 py-2 text-left", id === s.id ? "bg-accent/15" : "hover:bg-elevated")}
                    >
                      <Sparkle className={cn("mt-0.5 size-3.5 shrink-0", s.enabled ? "text-accent" : "text-subtle")} />
                      <span className="min-w-0 flex-1">
                        <span className={cn("block truncate text-[12.5px]", s.enabled ? "text-fg" : "text-subtle line-through")}>{s.name}</span>
                        <span className="block truncate text-[11px] text-subtle">{s.description}</span>
                      </span>
                      <span className="shrink-0 font-mono text-[10.5px] text-subtle">v{s.version}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </Card>
            <Card className="h-fit p-5">{id ? <SkillEditor key={id} id={id} onChanged={load} onDeleted={() => { load(); navigate({ view: "skills" }); }} /> : null}</Card>
          </div>
        )}
      </div>

      <InstallDialog
        preview={preview}
        onClose={() => setPreview(null)}
        onInstalled={(s) => {
          setPreview(null);
          load();
          navigate({ view: "skills", id: s.id });
        }}
      />
      <Dialog open={creating} onOpenChange={setCreating}>
        <DialogContent title="Nova skill" className="w-[min(720px,calc(100vw-32px))]">
          <form
            className="space-y-3 px-5 py-4"
            onSubmit={(e) => {
              e.preventDefault();
              void api
                .skillCreate(draft)
                .then((s) => {
                  setCreating(false);
                  setDraft({ name: "", description: "", body: "" });
                  load();
                  navigate({ view: "skills", id: s.id });
                })
                .catch((err) => toast.error("Skill inválida", { description: errorMessage(err) }));
            }}
          >
            <div>
              <Label htmlFor="ns-name">Nome</Label>
              <Input id="ns-name" autoFocus value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder="ex.: Tailwind Expert" />
            </div>
            <div>
              <Label htmlFor="ns-desc">Descrição (quando usar)</Label>
              <Input id="ns-desc" value={draft.description} onChange={(e) => setDraft({ ...draft, description: e.target.value })} />
            </div>
            <div>
              <Label htmlFor="ns-body">Instruções</Label>
              <Textarea id="ns-body" rows={10} className="font-mono text-[12px]" value={draft.body} onChange={(e) => setDraft({ ...draft, body: e.target.value })} />
            </div>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setCreating(false)}>
                Cancelar
              </Button>
              <Button type="submit" variant="primary">
                Criar
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
