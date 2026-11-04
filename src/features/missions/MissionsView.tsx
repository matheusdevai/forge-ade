import { useState } from "react";
import { Ban, ChevronDown, Plus, Target, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Dialog, DialogContent, EmptyState } from "@/components/ui";
import { errorMessage, type Mission } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useMissions, useWorkspaces } from "@/stores/data";
import { MissionStatusBadge, MODE_INFO } from "./MissionStatusBadge";

export function MissionsView() {
  const missions = useMissions((s) => s.items);
  const workspaces = useWorkspaces((s) => s.items);
  const { cancel, remove } = useMissions.getState();
  const openNewMission = useApp((s) => s.openNewMission);
  const openMission = (id: string) => useApp.getState().navigate({ view: "mission", id });
  const [expanded, setExpanded] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Mission | null>(null);

  const wsName = (id: string | null) => workspaces.find((w) => w.id === id)?.name;

  const run = async (fn: () => Promise<void>, ok: string) => {
    try {
      await fn();
      toast.success(ok);
    } catch (e) {
      toast.error("Operação falhou", { description: errorMessage(e) });
    }
  };

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-4xl px-8 py-8">
        <div className="mb-6 flex items-end">
          <div>
            <h1 className="text-xl font-semibold">Missões</h1>
            <p className="mt-1 text-xs text-muted">Briefing → plano revisado → tarefas executadas por agentes em worktrees isoladas → revisão → integração.</p>
          </div>
          <Button variant="primary" className="ml-auto" onClick={() => openNewMission()}>
            <Plus /> NEW MISSION
          </Button>
        </div>

        {missions.length === 0 ? (
          <EmptyState icon={<Target />} title="Nenhuma missão ainda" description="Crie a primeira missão descrevendo o que você quer construir." />
        ) : (
          <ul className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-surface">
            {missions.map((m) => {
              const open = expanded === m.id;
              return (
                <li key={m.id}>
                  <div className="flex w-full items-center gap-3 px-4 py-3 hover:bg-elevated/60">
                    <button type="button" aria-label={open ? "Recolher" : "Expandir"} onClick={() => setExpanded(open ? null : m.id)}>
                      <ChevronDown className={cn("size-3.5 shrink-0 text-subtle transition-transform", !open && "-rotate-90")} />
                    </button>
                    <button type="button" className="flex min-w-0 flex-1 items-center gap-3 text-left" onClick={() => openMission(m.id)}>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px] font-medium">{m.title}</div>
                      <div className="mt-0.5 text-[11.5px] text-subtle">
                        {wsName(m.workspaceId) ?? "Sem workspace"} · atualizada {relativeTime(m.updatedAt)}
                      </div>
                    </div>
                    <Badge>{MODE_INFO[m.mode].label}</Badge>
                    <MissionStatusBadge status={m.status} />
                    </button>
                  </div>
                  {open && (
                    <div className="border-t border-border bg-bg-2 px-11 py-3">
                      <p className="selectable text-[12.5px] leading-relaxed whitespace-pre-wrap text-muted">{m.briefing}</p>
                      <div className="mt-3 flex gap-2">
                        {!["completed", "cancelled"].includes(m.status) && (
                          <Button size="sm" variant="ghost" onClick={() => run(() => cancel(m.id), "Missão cancelada")}>
                            <Ban /> Cancelar missão
                          </Button>
                        )}
                        {["draft", "cancelled"].includes(m.status) && (
                          <Button size="sm" variant="danger" onClick={() => setConfirmDelete(m)}>
                            <Trash2 /> Excluir
                          </Button>
                        )}
                      </div>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <Dialog open={!!confirmDelete} onOpenChange={(o) => !o && setConfirmDelete(null)}>
        <DialogContent title="Excluir missão?" description="Esta ação não pode ser desfeita.">
          <div className="px-5 py-4 text-[13px]">
            “{confirmDelete?.title}” será removida permanentemente.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmDelete(null)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={async () => {
                  const m = confirmDelete;
                  setConfirmDelete(null);
                  if (m) await run(() => remove(m.id), "Missão excluída");
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
