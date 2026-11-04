import { useEffect, useMemo, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { ArrowDown, ArrowUp, ChevronDown, GitBranch, GitCommitHorizontal, Loader2, Plus, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import {
  Badge,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  EmptyState,
  Tooltip,
} from "@/components/ui";
import type { PaneProps } from "@/features/panes/registry";
import { api, errorMessage, type GitBlob, type GitBranch as Branch } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { ChangesTab, type ChangeTarget } from "./ChangesTab";
import { ConfirmDialog, type ConfirmRequest } from "./Confirm";
import { DiffView, type DiffAction } from "./DiffView";
import { FILES_CHANGED, GIT_CHANGED, gitRun, toWorkspacePath, useGit } from "./gitState";
import { rebuild, touches } from "./lineChanges";
import { BranchesTab, HistoryTab, NewBranchDialog, NewWorktreeDialog, StashTab, WorktreesTab, type CommitTarget } from "./RepoTabs";

type Tab = "changes" | "branches" | "history" | "stash" | "worktrees";
const TABS: { id: Tab; label: string }[] = [
  { id: "changes", label: "Alterações" },
  { id: "branches", label: "Branches" },
  { id: "history", label: "Histórico" },
  { id: "stash", label: "Stash" },
  { id: "worktrees", label: "Worktrees" },
];

type Target = ChangeTarget | CommitTarget;

function blobText(b: GitBlob): string | null {
  return b.binary || b.tooLarge ? null : (b.content ?? "");
}

export function GitPane({ workspaceId: ws }: PaneProps) {
  const entry = useGit((s) => s.byWs[ws]);
  const busy = useGit((s) => s.busy[ws] ?? null);
  const refresh = useGit((s) => s.refresh);
  const status = entry?.status ?? null;
  const [tab, setTab] = useState<Tab>("changes");
  const [target, setTarget] = useState<Target | null>(null);
  const [diff, setDiff] = useState<{ original: string | null; modified: string | null; notice: string | null; loading: boolean }>({
    original: null,
    modified: null,
    notice: null,
    loading: false,
  });
  const [reloadKey, setReloadKey] = useState(0);
  const [confirm, setConfirm] = useState<ConfirmRequest | null>(null);
  const [branches, setBranches] = useState<Branch[]>([]);
  const [newBranch, setNewBranch] = useState(false);
  const [worktreeDialog, setWorktreeDialog] = useState<{ open: boolean; branch: string | null }>({ open: false, branch: null });

  // Status refresh: mount, window focus, file saves, git ops, periodic while visible.
  useEffect(() => {
    void refresh(ws);
    const bump = () => setReloadKey((k) => k + 1);
    const onFocus = () => {
      void refresh(ws);
      bump();
    };
    const onEvent = (e: Event) => {
      if ((e as CustomEvent).detail?.workspaceId !== ws) return;
      void refresh(ws);
      bump();
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener(FILES_CHANGED, onEvent);
    window.addEventListener(GIT_CHANGED, onEvent);
    const t = setInterval(() => document.visibilityState === "visible" && void refresh(ws), 10_000);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener(FILES_CHANGED, onEvent);
      window.removeEventListener(GIT_CHANGED, onEvent);
      clearInterval(t);
    };
  }, [ws, refresh]);

  // Local branches for the header switcher.
  useEffect(() => {
    if (!status?.isRepo) return;
    api.gitBranches(ws).then(setBranches).catch(() => setBranches([]));
  }, [ws, status?.isRepo, status?.branch, reloadKey]);

  // Drop a selected change that no longer exists.
  useEffect(() => {
    if (target?.kind !== "change" || !status) return;
    const still = status.files.some((f) => f.path === target.path);
    if (!still) setTarget(null);
  }, [status, target]);

  // Load diff contents for the selection.
  useEffect(() => {
    if (!target) {
      setDiff({ original: null, modified: null, notice: null, loading: false });
      return;
    }
    let cancelled = false;
    setDiff((d) => ({ ...d, loading: true }));
    const head = status?.hasCommits ? "HEAD" : null;
    const empty: GitBlob = { exists: false, binary: false, tooLarge: false, content: "" };
    const get = (rev: string | null, path: string) => (rev ? api.gitFileContents(ws, rev, path) : Promise.resolve(empty));
    let pair: Promise<[GitBlob, GitBlob]>;
    if (target.kind === "commit") {
      pair = Promise.all([get(`${target.sha}^`, target.origPath ?? target.path), get(target.sha, target.path)]);
    } else {
      const orig = target.origPath ?? target.path;
      switch (target.area) {
        case "staged":
          pair = Promise.all([get(head, orig), get("INDEX", target.path)]);
          break;
        case "unstaged":
          pair = Promise.all([get("INDEX", target.path), get("WORKTREE", target.path)]);
          break;
        case "untracked":
          pair = Promise.all([Promise.resolve(empty), get("WORKTREE", target.path)]);
          break;
        case "conflict":
          pair = Promise.all([get(head, target.path), get("WORKTREE", target.path)]);
          break;
      }
    }
    pair
      .then(([a, b]) => {
        if (cancelled) return;
        const binary = a.binary || b.binary;
        const large = a.tooLarge || b.tooLarge;
        setDiff({
          original: binary || large ? null : blobText(a),
          modified: binary || large ? null : blobText(b),
          notice: binary ? "Arquivo binário — diff não disponível." : large ? "Arquivo grande demais para comparar (> 5 MB)." : null,
          loading: false,
        });
      })
      .catch((e) => !cancelled && setDiff({ original: null, modified: null, notice: errorMessage(e), loading: false }));
    return () => {
      cancelled = true;
    };
  }, [ws, target, reloadKey, status?.hasCommits]);

  const actions = useMemo<DiffAction[]>(() => {
    if (target?.kind !== "change") return [];
    const path = target.path;
    if (target.area === "unstaged") {
      return [
        {
          label: "Descartar seleção",
          variant: "ghost",
          run: (ctx) => {
            const rel = toWorkspacePath(status, path);
            if (!rel) {
              toast.error("Arquivo fora do workspace");
              return;
            }
            const picked = ctx.changes.filter((c) => touches(c, ctx.selection, "modified"));
            if (!picked.length) return toast.info("Selecione linhas alteradas no lado direito");
            setConfirm({
              title: "Descartar as linhas selecionadas?",
              description: path,
              body: `${picked.length} bloco(s) voltarão à versão do índice. As demais alterações do arquivo são mantidas.`,
              confirmLabel: "Descartar seleção",
              danger: true,
              run: async () => {
                try {
                  const content = rebuild(ctx.modified, ctx.original, ctx.changes, (c) => picked.includes(c), "modified");
                  const meta = await api.fsStat(ws, rel);
                  await api.fsWriteText(ws, rel, content, meta.mtimeMs);
                  const models = await import("@/features/editor/models").catch(() => null);
                  await models?.checkExternalChange(ws, rel);
                } catch (e) {
                  toast.error("Descartar falhou", { description: errorMessage(e) });
                }
                await refresh(ws);
                setReloadKey((k) => k + 1);
              },
            });
          },
        },
        {
          label: "Stage seleção",
          run: (ctx) => {
            const picked = ctx.changes.filter((c) => touches(c, ctx.selection, "modified"));
            if (!picked.length) return toast.info("Selecione linhas alteradas no lado direito");
            const content = rebuild(ctx.original, ctx.modified, ctx.changes, (c) => picked.includes(c), "original");
            void gitRun(ws, "Stage parcial", () => api.gitStageContent(ws, path, content), { success: `${picked.length} bloco(s) em stage` }).then(() =>
              setReloadKey((k) => k + 1),
            );
          },
        },
      ];
    }
    if (target.area === "staged") {
      return [
        {
          label: "Unstage seleção",
          run: (ctx) => {
            const picked = ctx.changes.filter((c) => touches(c, ctx.selection, "modified"));
            if (!picked.length) return toast.info("Selecione linhas alteradas no lado direito");
            const content = rebuild(ctx.modified, ctx.original, ctx.changes, (c) => picked.includes(c), "modified");
            void gitRun(ws, "Unstage parcial", () => api.gitStageContent(ws, path, content), { success: `${picked.length} bloco(s) fora do stage` }).then(() =>
              setReloadKey((k) => k + 1),
            );
          },
        },
      ];
    }
    return [];
  }, [target, ws, status, refresh]);

  if (!status) {
    return entry?.error ? (
      <EmptyState className="h-full" title="Git indisponível" description={entry.error} />
    ) : (
      <div className="flex h-full items-center justify-center text-subtle">
        <Loader2 className="size-4 animate-spin" />
      </div>
    );
  }

  if (!status.isRepo) {
    return (
      <EmptyState
        className="h-full"
        icon={<GitBranch />}
        title="Este workspace não é um repositório Git"
        description="Inicialize um repositório para versionar o projeto."
        action={
          <Button size="sm" onClick={() => void gitRun(ws, "Inicializar repositório", () => api.gitInit(ws), { success: "Repositório inicializado" })}>
            Inicializar repositório
          </Button>
        }
      />
    );
  }

  const localBranches = branches.filter((b) => !b.remote && !b.current);
  const diffTitle = target ? target.path.slice(target.path.lastIndexOf("/") + 1) : "";
  const diffSubtitle =
    target?.kind === "commit"
      ? `commit ${target.short}`
      : target
        ? { staged: "HEAD ↔ stage", unstaged: "stage ↔ arquivo", untracked: "arquivo novo", conflict: "HEAD ↔ arquivo (conflito)" }[target.area]
        : "";

  return (
    <div className="flex h-full min-w-0 flex-col">
      {/* Header */}
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-border px-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="sm" variant="ghost" className="h-7 max-w-[45%] gap-1.5 px-2">
              <GitBranch className="text-accent" />
              <span className="truncate font-mono text-[12px]">{status.branch ?? `HEAD ${status.oid?.slice(0, 7) ?? ""}`}</span>
              <ChevronDown className="text-subtle" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="max-h-[60vh] overflow-auto">
            <DropdownMenuItem onSelect={() => setNewBranch(true)}>
              <Plus /> Nova branch…
            </DropdownMenuItem>
            {localBranches.length > 0 && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>Trocar para</DropdownMenuLabel>
                {localBranches.slice(0, 30).map((b) => (
                  <DropdownMenuItem
                    key={b.name}
                    onSelect={() => void gitRun(ws, "Trocar de branch", () => api.gitSwitch(ws, b.name, false), { worktreeChanged: true, success: `Branch ${b.name} ativada` })}
                  >
                    <GitBranch /> {b.name}
                  </DropdownMenuItem>
                ))}
              </>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => setTab("branches")}>Gerenciar branches</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        {status.upstream && (
          <span className="truncate text-[11px] text-subtle" title={`upstream: ${status.upstream}`}>
            → {status.upstream}
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          {busy && (
            <span className="mr-1 flex items-center gap-1 text-[11px] text-subtle">
              <Loader2 className="size-3 animate-spin" /> {busy}…
            </span>
          )}
          <Tooltip content="Buscar do remoto (fetch)">
            <Button size="icon" variant="ghost" className="size-7" aria-label="Fetch" disabled={!!busy} onClick={() => void gitRun(ws, "Fetch", () => api.gitFetch(ws), { success: "Fetch concluído" })}>
              <RefreshCw />
            </Button>
          </Tooltip>
          <Tooltip content="Pull (somente fast-forward)">
            <Button size="sm" variant="ghost" className="h-7 px-2" disabled={!!busy || !status.upstream} onClick={() => void gitRun(ws, "Pull", () => api.gitPull(ws), { worktreeChanged: true, success: "Pull concluído" })}>
              <ArrowDown /> {status.behind > 0 && status.behind}
            </Button>
          </Tooltip>
          <Tooltip content={status.upstream ? "Push" : "Publicar branch (push -u)"}>
            <Button
              size="sm"
              variant={status.ahead > 0 || (!status.upstream && status.hasCommits) ? "secondary" : "ghost"}
              className="h-7 px-2"
              disabled={!!busy || !status.branch || !status.hasCommits}
              onClick={() => void gitRun(ws, "Push", () => api.gitPush(ws), { success: "Push concluído" })}
            >
              <ArrowUp /> {status.ahead > 0 ? status.ahead : !status.upstream ? "Publicar" : ""}
            </Button>
          </Tooltip>
        </div>
      </div>

      <Group orientation="horizontal" className="min-h-0 flex-1">
        <Panel id="git-side" defaultSize="38" minSize="24">
          <div className="flex h-full min-w-0 flex-col border-r border-border">
            <div className="flex h-8 shrink-0 items-stretch overflow-x-auto border-b border-border px-1" role="tablist">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  role="tab"
                  aria-selected={tab === t.id}
                  onClick={() => setTab(t.id)}
                  className={cn("relative shrink-0 px-2 text-[11.5px]", tab === t.id ? "text-fg" : "text-subtle hover:text-muted")}
                >
                  {t.label}
                  {t.id === "changes" && status.files.length > 0 && <Badge className="ml-1">{status.files.length}</Badge>}
                  {tab === t.id && <span className="absolute inset-x-1 bottom-0 h-px bg-accent" />}
                </button>
              ))}
            </div>
            <div className="min-h-0 flex-1">
              {tab === "changes" && <ChangesTab ws={ws} status={status} selected={target?.kind === "change" ? target : null} onSelect={setTarget} confirm={setConfirm} />}
              {tab === "branches" && <BranchesTab ws={ws} status={status} confirm={setConfirm} onNewWorktree={(b) => setWorktreeDialog({ open: true, branch: b })} />}
              {tab === "history" && <HistoryTab ws={ws} selected={target?.kind === "commit" ? target : null} onSelect={setTarget} />}
              {tab === "stash" && <StashTab ws={ws} status={status} confirm={setConfirm} />}
              {tab === "worktrees" && <WorktreesTab ws={ws} status={status} confirm={setConfirm} onNew={() => setWorktreeDialog({ open: true, branch: null })} />}
            </div>
          </div>
        </Panel>
        <Separator />
        <Panel id="git-diff" minSize="30">
          {target ? (
            <DiffView
              title={diffTitle}
              subtitle={diffSubtitle}
              original={diff.original}
              modified={diff.modified}
              path={target.path}
              loading={diff.loading}
              notice={diff.notice}
              actions={actions}
            />
          ) : (
            <EmptyState
              className="h-full"
              icon={<GitCommitHorizontal />}
              title="Selecione um arquivo"
              description="O diff aparece aqui. Selecione linhas no lado direito para fazer stage, unstage ou descartar só parte das alterações."
            />
          )}
        </Panel>
      </Group>

      <ConfirmDialog request={confirm} onClose={() => setConfirm(null)} />
      <NewBranchDialog ws={ws} status={status} open={newBranch} onClose={() => setNewBranch(false)} />
      <NewWorktreeDialog ws={ws} status={status} branch={worktreeDialog.branch} open={worktreeDialog.open} onClose={() => setWorktreeDialog({ open: false, branch: null })} />
    </div>
  );
}
