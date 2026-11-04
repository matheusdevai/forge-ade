import { useEffect, useState } from "react";
import { Info } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, DialogContent, Label, Input, Textarea } from "@/components/ui";
import { api, errorMessage, type MissionMode } from "@/lib/ipc";
import { useLibrary } from "@/stores/library";
import { cn } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useMissions, useWorkspaces } from "@/stores/data";
import { MODE_INFO } from "./MissionStatusBadge";

/** A short title from a spoken/typed briefing (first clause, capitalized). */
export function titleFromBriefing(b: string): string {
  const first = b.trim().split(/[.!?\n]/)[0] ?? "";
  const t = first.length > 60 ? `${first.slice(0, 57).trimEnd()}…` : first;
  return t ? t[0]!.toUpperCase() + t.slice(1) : "";
}

export function NewMissionDialog() {
  const { open, workspaceId, briefing: preset } = useApp((s) => s.newMission);
  const close = useApp((s) => s.closeNewMission);
  const workspaces = useWorkspaces((s) => s.items);
  const create = useMissions((s) => s.create);

  const [title, setTitle] = useState("");
  const [briefing, setBriefing] = useState("");
  const [ws, setWs] = useState<string>("");
  const [mode, setMode] = useState<MissionMode>("squad");
  const squads = useLibrary((s) => s.squads);
  const [squadId, setSquadId] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open) {
      setTitle(preset ? titleFromBriefing(preset) : "");
      setBriefing(preset ?? "");
      setMode("squad");
      setSquadId("");
      setWs(workspaceId ?? "");
      if (!useLibrary.getState().loaded) void useLibrary.getState().refresh().catch(() => undefined);
    }
  }, [open, workspaceId, preset]);

  const canSubmit = title.trim().length > 0 && briefing.trim().length > 0 && !saving;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    setSaving(true);
    try {
      const m = await create({ title, briefing, mode, workspaceId: ws || null });
      if (squadId) await api.missionSetSquad(m.id, squadId);
      toast.success("Missão criada", { description: "Gere o plano com IA ou monte as tarefas manualmente." });
      close();
      useApp.getState().navigate({ view: "mission", id: m.id });
    } catch (err) {
      toast.error("Não foi possível criar a missão", { description: errorMessage(err) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent title="Nova missão" description="Descreva o que você quer construir. O FORGE registra o briefing para o Maestro.">
        <form onSubmit={submit} className="space-y-4 px-5 py-4">
          <div>
            <Label htmlFor="m-title">Nome</Label>
            <Input id="m-title" autoFocus maxLength={200} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Ex.: SaaS com autenticação e pagamentos" />
          </div>
          <div>
            <Label htmlFor="m-brief">Briefing</Label>
            <Textarea
              id="m-brief"
              rows={6}
              maxLength={20000}
              value={briefing}
              onChange={(e) => setBriefing(e.target.value)}
              placeholder="Desenvolva uma aplicação SaaS com autenticação, dashboard, pagamentos e gerenciamento de usuários."
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void submit(e);
              }}
            />
          </div>
          <div>
            <Label htmlFor="m-ws">Workspace</Label>
            <select
              id="m-ws"
              value={ws}
              onChange={(e) => setWs(e.target.value)}
              className="h-9 w-full rounded-md border border-border bg-bg-2 px-2.5 text-[13px] text-fg focus:border-accent/60 focus:outline-none"
            >
              <option value="">Sem workspace (definir depois)</option>
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <Label htmlFor="m-squad">Squad</Label>
            <select
              id="m-squad"
              value={squadId}
              onChange={(e) => {
                setSquadId(e.target.value);
                const s = squads.find((x) => x.id === e.target.value);
                if (s) setMode(s.defaultMode);
              }}
              className="h-9 w-full rounded-md border border-border bg-bg-2 px-2.5 text-[13px] text-fg focus:border-accent/60 focus:outline-none"
            >
              <option value="">Sem squad (o planejador escolhe entre todos os agentes)</option>
              {squads.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name} — {s.flow}
                </option>
              ))}
            </select>
          </div>
          <div>
            <Label>Modo</Label>
            <div role="radiogroup" className="grid grid-cols-3 gap-2">
              {(Object.keys(MODE_INFO) as MissionMode[]).map((m) => (
                <button
                  key={m}
                  type="button"
                  role="radio"
                  aria-checked={mode === m}
                  onClick={() => setMode(m)}
                  className={cn(
                    "rounded-lg border p-2.5 text-left transition-colors",
                    mode === m ? "border-accent/60 bg-accent/10" : "border-border bg-bg-2 hover:border-border-strong",
                  )}
                >
                  <div className="text-xs font-semibold tracking-wide">{MODE_INFO[m].label.toUpperCase()}</div>
                  <div className="mt-1 text-[11px] leading-snug text-muted">{MODE_INFO[m].description}</div>
                </button>
              ))}
            </div>
          </div>
          <p className="flex gap-2 rounded-md border border-border bg-bg-2 p-2.5 text-[11.5px] leading-relaxed text-muted">
            <Info className="mt-0.5 size-3.5 shrink-0 text-accent" />
            Depois de criar, um agente propõe o plano (tarefas, agentes e dependências) para você revisar. Cada tarefa roda numa worktree isolada e nada entra na sua branch sem aprovação.
          </p>
          <div className="flex justify-end gap-2 pt-1">
            <Button
              type="button"
              variant="ghost"
              className="mr-auto"
              onClick={() => {
                close();
                useApp.getState().navigate({ view: "library", tab: "recipes" });
              }}
            >
              Usar uma receita…
            </Button>
            <Button type="button" variant="ghost" onClick={close}>
              Cancelar
            </Button>
            <Button type="submit" variant="primary" disabled={!canSubmit}>
              Salvar missão
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
