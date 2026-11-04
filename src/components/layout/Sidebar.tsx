import { Activity, Bot, Brain, Clapperboard, Library, Timer, Plug, Sparkle, Users, FolderOpen, FolderPlus, Home, MoreHorizontal, Pin, PinOff, Settings, Target, Trash2 } from "lucide-react";
import { useProviders } from "@/stores/providers";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Logo, Mascot, hasMascot } from "@/brand";
import { Button, Dialog, DialogContent, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger, Tooltip } from "@/components/ui";
import { openWorkspace, pickAndOpenWorkspace } from "@/features/workspace/actions";
import { revealLabel } from "@/features/workspace/WorkspaceView";
import { api, errorMessage, type Workspace } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useMissions, useWorkspaces } from "@/stores/data";

function NavItem({ active, icon: Icon, label, onClick, trailing }: { active: boolean; icon: typeof Home; label: string; onClick: () => void; trailing?: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-[13px] transition-colors",
        active ? "bg-elevated text-fg" : "text-muted hover:bg-elevated/60 hover:text-fg",
      )}
    >
      <Icon className={cn("size-4", active ? "text-accent" : "text-subtle")} />
      {label}
      {trailing && <span className="ml-auto">{trailing}</span>}
    </button>
  );
}

export function Sidebar() {
  const route = useApp((s) => s.route);
  const navigate = useApp((s) => s.navigate);
  const workspaces = useWorkspaces((s) => s.items);
  const openCount = useMissions((s) => s.items.filter((m) => !["completed", "cancelled", "failed"].includes(m.status)).length);
  const [removing, setRemoving] = useState<Workspace | null>(null);
  const [runningCount, setRunningCount] = useState(0);
  const [suggestedCount, setSuggestedCount] = useState(0);
  useEffect(() => {
    let alive = true;
    const load = () => void api.memoryCounts(null).then((c) => alive && setSuggestedCount(c.suggested)).catch(() => undefined);
    load();
    const t = setInterval(load, 10000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [route]);
  useEffect(() => {
    let alive = true;
    const load = () => api.agentRunsList(null, "running", 50).then((r) => alive && setRunningCount(r.length)).catch(() => undefined);
    void load();
    const t = setInterval(load, 4000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  const readyCount = useProviders((s) => s.items.filter((p) => p.installed && p.auth.state === "logged_in").length);

  const act = (fn: () => Promise<unknown>) => fn().catch((e) => toast.error("Operação falhou", { description: errorMessage(e) }));

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-bg-2">
      <div className="flex h-12 items-center px-4" data-tauri-drag-region>
        <Logo className="h-5" />
      </div>

      <nav className="space-y-0.5 px-2">
        <NavItem active={route.view === "dashboard"} icon={Home} label="Dashboard" onClick={() => navigate({ view: "dashboard" })} />
        <NavItem
          active={route.view === "missions" || route.view === "mission"}
          icon={Target}
          label="Missões"
          onClick={() => navigate({ view: "missions" })}
          trailing={openCount > 0 ? <span className="text-[11px] text-subtle">{openCount}</span> : null}
        />
        <NavItem active={route.view === "agents"} icon={Users} label="Agentes" onClick={() => navigate({ view: "agents" })} trailing={runningCount > 0 ? <span className="text-[11px] text-success">{runningCount} ativos</span> : null} />
        <NavItem
          active={route.view === "memory"}
          icon={Brain}
          label="Memória"
          onClick={() => navigate({ view: "memory" })}
          trailing={suggestedCount > 0 ? <span className="text-[11px] text-accent" title="Sugestões aguardando revisão">{suggestedCount} novas</span> : null}
        />
        <NavItem active={route.view === "observability"} icon={Activity} label="Observabilidade" onClick={() => navigate({ view: "observability" })} />
        <NavItem active={route.view === "autopilot"} icon={Timer} label="Autopilot" onClick={() => navigate({ view: "autopilot" })} />
        <NavItem active={route.view === "media"} icon={Clapperboard} label="Mídia" onClick={() => navigate({ view: "media" })} />
        <NavItem active={route.view === "mcp"} icon={Plug} label="MCP" onClick={() => navigate({ view: "mcp" })} />
        <NavItem active={route.view === "skills"} icon={Sparkle} label="Skills" onClick={() => navigate({ view: "skills" })} />
        <NavItem active={route.view === "library"} icon={Library} label="Squads & Receitas" onClick={() => navigate({ view: "library" })} />
        <NavItem
          active={route.view === "providers"}
          icon={Bot}
          label="Providers"
          onClick={() => navigate({ view: "providers" })}
          trailing={readyCount > 0 ? <span className="text-[11px] text-subtle">{readyCount}</span> : null}
        />
      </nav>

      <div className="mt-5 flex items-center px-4 pb-1">
        <span className="text-[10.5px] font-medium uppercase tracking-wider text-subtle">Workspaces</span>
        <Tooltip content="Abrir workspace">
          <Button variant="ghost" size="icon" className="ml-auto size-6" aria-label="Abrir workspace" onClick={() => void pickAndOpenWorkspace()}>
            <FolderPlus />
          </Button>
        </Tooltip>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-2 pb-2">
        {workspaces.length === 0 && <p className="px-2.5 py-2 text-xs text-subtle">Nenhum workspace.</p>}
        {workspaces.map((w) => {
          const active = route.view === "workspace" && route.id === w.id;
          return (
            <div key={w.id} className={cn("group flex items-center rounded-md", active ? "bg-elevated" : "hover:bg-elevated/60")}>
              <button type="button" onClick={() => void openWorkspace(w.id)} className="flex h-8 min-w-0 flex-1 items-center gap-2.5 px-2.5 text-left">
                <span
                  className={cn(
                    "flex size-[18px] shrink-0 items-center justify-center rounded text-[9.5px] font-semibold uppercase",
                    active ? "bg-accent/20 text-accent" : "bg-surface text-muted",
                  )}
                >
                  {w.name.slice(0, 1)}
                </span>
                <span className={cn("truncate text-[13px]", active ? "text-fg" : "text-muted")}>{w.name}</span>
                {w.pinned && <Pin className="size-3 shrink-0 text-subtle" />}
              </button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="icon" className="mr-0.5 size-6 opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100" aria-label={`Ações de ${w.name}`}>
                    <MoreHorizontal />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" side="right">
                  <DropdownMenuItem onSelect={() => act(() => useWorkspaces.getState().setPinned(w.id, !w.pinned))}>
                    {w.pinned ? <PinOff /> : <Pin />} {w.pinned ? "Desafixar" : "Fixar no topo"}
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => act(() => api.workspacesReveal(w.id))}>
                    <FolderOpen /> {revealLabel}
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem className="text-error" onSelect={() => setRemoving(w)}>
                    <Trash2 /> Remover da lista
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          );
        })}
      </div>

      <div className="border-t border-border p-2">
        <NavItem
          active={route.view === "settings"}
          icon={Settings}
          label="Configurações"
          onClick={() => navigate({ view: "settings", section: "general" })}
          trailing={hasMascot ? <Mascot state="online" size={20} className="animate-none" /> : null}
        />
      </div>

      <Dialog open={!!removing} onOpenChange={(o) => !o && setRemoving(null)}>
        <DialogContent title="Remover workspace da lista?" description="Os arquivos do projeto não serão apagados. As memórias deste workspace (e das missões e agentes ligados só a ele) serão excluídas.">
          <div className="px-5 py-4 text-[13px] text-muted">
            “{removing?.name}” deixará de aparecer no FORGE, junto com seu layout de panes salvo. Missões vinculadas são mantidas sem workspace.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setRemoving(null)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={async () => {
                  const w = removing;
                  setRemoving(null);
                  if (!w) return;
                  await act(async () => {
                    await useWorkspaces.getState().remove(w.id);
                    await useMissions.getState().refresh();
                    const r = useApp.getState().route;
                    if (r.view === "workspace" && r.id === w.id) navigate({ view: "dashboard" });
                  });
                }}
              >
                Remover
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </aside>
  );
}
