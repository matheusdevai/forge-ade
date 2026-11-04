import { useState } from "react";
import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, DialogContent, Input, Label, Textarea } from "@/components/ui";
import { api, errorMessage, type MissionTask, type TaskInput } from "@/lib/ipc";
import { useAgents } from "@/stores/agents";

export function toInputs(tasks: MissionTask[]): TaskInput[] {
  const keyOf = new Map(tasks.map((t) => [t.id, t.key]));
  return tasks.map((t) => ({
    key: t.key,
    title: t.title,
    description: t.description,
    acceptance: t.acceptance,
    profileId: t.profileId,
    dependsOn: t.dependsOn.map((id) => keyOf.get(id) ?? id),
  }));
}

function nextKey(rows: TaskInput[]): string {
  const used = new Set(rows.map((r) => r.key));
  for (let i = rows.length + 1; ; i++) if (!used.has(`t${i}`)) return `t${i}`;
}

/** Edits the mission plan. Dependencies may only point to tasks listed above (keeps it acyclic). */
export function PlanEditor({ missionId, initial, open, onClose }: { missionId: string; initial: TaskInput[]; open: boolean; onClose: () => void }) {
  const profiles = useAgents((s) => s.profiles);
  const [rows, setRows] = useState<TaskInput[]>(initial);
  const [saving, setSaving] = useState(false);

  const update = (i: number, patch: Partial<TaskInput>) => setRows((r) => r.map((x, j) => (j === i ? { ...x, ...patch } : x)));
  const move = (i: number, d: -1 | 1) =>
    setRows((r) => {
      const j = i + d;
      if (j < 0 || j >= r.length) return r;
      const out = [...r];
      [out[i], out[j]] = [out[j]!, out[i]!];
      // Drop dependencies that would now point below.
      return out.map((x, idx) => ({ ...x, dependsOn: x.dependsOn.filter((k) => out.findIndex((y) => y.key === k) < idx) }));
    });
  const remove = (i: number) =>
    setRows((r) => {
      const key = r[i]!.key;
      return r.filter((_, j) => j !== i).map((x) => ({ ...x, dependsOn: x.dependsOn.filter((k) => k !== key) }));
    });
  const add = () =>
    setRows((r) => [...r, { key: nextKey(r), title: "", description: "", acceptance: "", profileId: profiles.find((p) => p.id === "builtin:backend")?.id ?? profiles[0]?.id ?? null, dependsOn: [] }]);

  const save = async () => {
    setSaving(true);
    try {
      await api.missionSaveTasks(missionId, rows);
      toast.success("Plano salvo");
      onClose();
    } catch (e) {
      toast.error("Plano inválido", { description: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent title="Editar plano" description="Cada tarefa roda com um agente, numa worktree própria. Dependências só podem apontar para tarefas acima." className="w-[min(880px,calc(100vw-32px))]">
        <div className="max-h-[65vh] space-y-3 overflow-auto px-5 py-4">
          {rows.map((r, i) => (
            <div key={r.key} className="rounded-lg border border-border bg-bg-2 p-3">
              <div className="flex items-center gap-2">
                <span className="w-8 shrink-0 font-mono text-[11px] text-subtle">{r.key}</span>
                <Input className="h-8 flex-1" value={r.title} placeholder="Título da tarefa" onChange={(e) => update(i, { title: e.target.value })} aria-label="Título" />
                <select
                  value={r.profileId ?? ""}
                  onChange={(e) => update(i, { profileId: e.target.value || null })}
                  aria-label="Agente"
                  className="h-8 w-48 rounded-md border border-border bg-bg px-2 text-[12px] text-fg focus:outline-none"
                >
                  {profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
                <Button size="icon" variant="ghost" aria-label="Subir" disabled={i === 0} onClick={() => move(i, -1)}>
                  <ArrowUp />
                </Button>
                <Button size="icon" variant="ghost" aria-label="Descer" disabled={i === rows.length - 1} onClick={() => move(i, 1)}>
                  <ArrowDown />
                </Button>
                <Button size="icon" variant="ghost" aria-label="Remover" onClick={() => remove(i)}>
                  <Trash2 />
                </Button>
              </div>
              <div className="mt-2 grid grid-cols-2 gap-2">
                <div>
                  <Label>Instruções para o agente</Label>
                  <Textarea rows={4} value={r.description} onChange={(e) => update(i, { description: e.target.value })} />
                </div>
                <div>
                  <Label>Critérios de aceite</Label>
                  <Textarea rows={4} value={r.acceptance} onChange={(e) => update(i, { acceptance: e.target.value })} />
                </div>
              </div>
              {i > 0 && (
                <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px]">
                  <span className="text-subtle">Depende de:</span>
                  {rows.slice(0, i).map((d) => (
                    <label key={d.key} className="flex items-center gap-1 text-muted">
                      <input
                        type="checkbox"
                        className="accent-[#579bff]"
                        checked={r.dependsOn.includes(d.key)}
                        onChange={(e) => update(i, { dependsOn: e.target.checked ? [...r.dependsOn, d.key] : r.dependsOn.filter((k) => k !== d.key) })}
                      />
                      {d.title || d.key}
                    </label>
                  ))}
                </div>
              )}
            </div>
          ))}
          <Button variant="ghost" onClick={add} disabled={rows.length >= 20}>
            <Plus /> Adicionar tarefa
          </Button>
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="primary" disabled={saving || rows.length === 0} onClick={() => void save()}>
            Salvar plano
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
