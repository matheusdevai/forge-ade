import { useEffect, useMemo, useState } from "react";
import { FolderOpen, GitBranch, Plus, Target } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, EmptyState } from "@/components/ui";
import { MissionStatusBadge } from "@/features/missions/MissionStatusBadge";
import { api, errorMessage, type GitInfo } from "@/lib/ipc";
import { prettyPath, relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useMissions, useWorkspaces } from "@/stores/data";
import type { PaneProps } from "../registry";

export function OverviewPane({ workspaceId }: PaneProps) {
  const ws = useWorkspaces((s) => s.items.find((w) => w.id === workspaceId));
  // Select the stable array, filter in render: a selector returning a new
  // array every call makes zustand/React loop forever (React error #185).
  const allMissions = useMissions((s) => s.items);
  const missions = useMemo(() => allMissions.filter((m) => m.workspaceId === workspaceId), [allMissions, workspaceId]);
  const openNewMission = useApp((s) => s.openNewMission);
  const [git, setGit] = useState<GitInfo | null>(null);

  useEffect(() => {
    api.gitInfo(workspaceId).then(setGit).catch(() => setGit(null));
  }, [workspaceId]);

  if (!ws) return null;

  return (
    <div className="h-full overflow-auto p-4">
      <h2 className="text-[15px] font-semibold">{ws.name}</h2>
      <p className="selectable mt-0.5 truncate font-mono text-[11.5px] text-muted" title={ws.path}>
        {prettyPath(ws.path)}
      </p>

      <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
        <dt className="text-subtle">Git</dt>
        <dd>
          {git?.isRepo ? (
            <span className="inline-flex items-center gap-1 text-fg">
              <GitBranch className="size-3 text-accent" />
              {git.branch ?? `HEAD destacado (${git.detachedHead})`}
            </span>
          ) : (
            <span className="text-muted">Não é um repositório Git</span>
          )}
        </dd>
        <dt className="text-subtle">Adicionado</dt>
        <dd className="text-muted">{relativeTime(ws.createdAt)}</dd>
        <dt className="text-subtle">Último acesso</dt>
        <dd className="text-muted">{ws.lastOpenedAt ? relativeTime(ws.lastOpenedAt) : "—"}</dd>
      </dl>

      <div className="mt-4 flex flex-wrap gap-2">
        <Button size="sm" onClick={() => openNewMission(workspaceId)}>
          <Plus /> Nova missão
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => api.workspacesReveal(workspaceId).catch((e) => toast.error("Falha ao abrir no gerenciador", { description: errorMessage(e) }))}
        >
          <FolderOpen /> Mostrar no Finder
        </Button>
      </div>

      <h3 className="mt-6 mb-2 text-[10.5px] font-medium uppercase tracking-wider text-subtle">Missões deste workspace</h3>
      {missions.length === 0 ? (
        <EmptyState icon={<Target />} title="Nenhuma missão" description="Crie uma missão para registrar o briefing deste projeto." className="py-4" />
      ) : (
        <ul className="space-y-1.5">
          {missions.slice(0, 8).map((m) => (
            <li key={m.id} className="flex items-center gap-2 rounded-md border border-border bg-surface px-2.5 py-2">
              <span className="min-w-0 flex-1 truncate text-[12.5px]">{m.title}</span>
              <Badge>{m.mode}</Badge>
              <MissionStatusBadge status={m.status} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
