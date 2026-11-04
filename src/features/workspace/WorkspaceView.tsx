import { useEffect, useState } from "react";
import { FolderOpen, GitBranch, LayoutGrid, Plus, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger, EmptyState, Tooltip } from "@/components/ui";
import { addPane } from "@/features/panes/layout";
import { PaneGrid } from "@/features/panes/PaneGrid";
import { PaneKindMenuItems } from "@/features/panes/PaneFrame";
import { SessionTabs } from "@/features/panes/SessionTabs";
import { useLayout } from "@/features/panes/store";
import { GIT_CHANGED } from "@/features/git/gitState";
import { api, errorMessage, type GitInfo } from "@/lib/ipc";
import { isMac, prettyPath } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useWorkspaces } from "@/stores/data";

export const revealLabel = isMac ? "Mostrar no Finder" : "Mostrar na pasta";

export function WorkspaceView({ id }: { id: string }) {
  const ws = useWorkspaces((s) => s.items.find((w) => w.id === id));
  const load = useLayout((s) => s.load);
  const apply = useLayout((s) => s.apply);
  const reset = useLayout((s) => s.reset);
  const openNewMission = useApp((s) => s.openNewMission);
  const [git, setGit] = useState<GitInfo | null>(null);

  useEffect(() => {
    void load(id);
  }, [id, load]);

  // Branch badge follows git operations made anywhere in the app.
  useEffect(() => {
    const read = () => api.gitInfo(id).then(setGit).catch(() => setGit(null));
    void read();
    const onChange = (e: Event) => (e as CustomEvent).detail?.workspaceId === id && void read();
    window.addEventListener(GIT_CHANGED, onChange);
    window.addEventListener("focus", read);
    return () => {
      window.removeEventListener(GIT_CHANGED, onChange);
      window.removeEventListener("focus", read);
    };
  }, [id]);

  if (!ws) {
    return <EmptyState className="h-full" title="Workspace não encontrado" description="Ele pode ter sido removido da lista." />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-3 border-b border-border px-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="truncate text-[13.5px] font-semibold">{ws.name}</h1>
            {git?.isRepo && (
              <span className="inline-flex items-center gap-1 rounded border border-border bg-surface px-1.5 py-px font-mono text-[10.5px] text-muted">
                <GitBranch className="size-3 text-accent" />
                {git.branch ?? git.detachedHead}
              </span>
            )}
          </div>
        </div>
        <span className="selectable hidden min-w-0 truncate font-mono text-[11px] text-subtle lg:block" title={ws.path}>
          {prettyPath(ws.path)}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="sm" variant="ghost">
                <LayoutGrid /> Pane
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <PaneKindMenuItems onPick={(kind) => apply((d) => addPane(d, kind))} />
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={reset}>
                <RotateCcw /> Restaurar layout padrão
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Tooltip content={revealLabel}>
            <Button
              size="icon"
              variant="ghost"
              aria-label={revealLabel}
              onClick={() => api.workspacesReveal(id).catch((e) => toast.error("Falha ao abrir", { description: errorMessage(e) }))}
            >
              <FolderOpen />
            </Button>
          </Tooltip>
          <Button size="sm" onClick={() => openNewMission(id)}>
            <Plus /> Nova missão
          </Button>
        </div>
      </div>
      <SessionTabs />
      <div className="min-h-0 flex-1">
        <PaneGrid workspaceId={id} />
      </div>
    </div>
  );
}
