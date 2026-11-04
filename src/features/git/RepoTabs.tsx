import { useCallback, useEffect, useState } from "react";
import { Archive, ChevronRight, FolderOpen, GitBranch, GitMerge, Loader2, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Dialog, DialogContent, EmptyState, Input, Label, Tooltip } from "@/components/ui";
import { openWorkspace } from "@/features/workspace/actions";
import { api, errorMessage, type GitBranch as Branch, type GitCommit, type GitCommitFile, type GitStash, type GitStatus, type GitWorktree } from "@/lib/ipc";
import { cn, prettyPath, relativeTime } from "@/lib/utils";
import { useWorkspaces } from "@/stores/data";
import type { ConfirmRequest } from "./Confirm";
import { GIT_CHANGED, gitRun } from "./gitState";

export type CommitTarget = { kind: "commit"; sha: string; short: string; path: string; origPath: string | null; status: string };

/** Loads data for a tab and reloads it whenever the repo changes. */
function useGitData<T>(ws: string, load: (ws: string) => Promise<T>): { data: T | null; error: string | null; reload: () => void } {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(() => {
    load(ws)
      .then((d) => {
        setData(d);
        setError(null);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [ws, load]);
  useEffect(() => {
    reload();
    const onChange = (e: Event) => (e as CustomEvent).detail?.workspaceId === ws && reload();
    window.addEventListener(GIT_CHANGED, onChange);
    return () => window.removeEventListener(GIT_CHANGED, onChange);
  }, [ws, reload]);
  return { data, error, reload };
}

function Loading({ error }: { error: string | null }) {
  return error ? (
    <p className="p-3 text-[12px] text-error">{error}</p>
  ) : (
    <div className="flex justify-center p-4 text-subtle">
      <Loader2 className="size-4 animate-spin" />
    </div>
  );
}

// ── Branches ──────────────────────────────────────────────────────────────

export function NewBranchDialog({ ws, status, open, onClose, initialBase }: { ws: string; status: GitStatus; open: boolean; onClose: () => void; initialBase?: string }) {
  const [name, setName] = useState("");
  const [base, setBase] = useState("");
  useEffect(() => {
    if (open) {
      setName("");
      setBase(initialBase ?? status.branch ?? "HEAD");
    }
  }, [open, initialBase, status.branch]);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent title="Nova branch" description="A branch é criada e ativada.">
        <form
          className="space-y-4 px-5 py-4"
          onSubmit={(e) => {
            e.preventDefault();
            const n = name.trim();
            if (!n) return;
            onClose();
            void gitRun(ws, "Criar branch", () => api.gitSwitch(ws, n, true, base || null), { worktreeChanged: true, success: `Branch ${n} criada` });
          }}
        >
          <div>
            <Label htmlFor="nb-name">Nome</Label>
            <Input id="nb-name" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="feat/minha-feature" />
          </div>
          <div>
            <Label htmlFor="nb-base">A partir de</Label>
            <Input id="nb-base" value={base} onChange={(e) => setBase(e.target.value)} placeholder="HEAD" />
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancelar
            </Button>
            <Button type="submit" variant="primary" disabled={!name.trim()}>
              Criar e ativar
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function BranchesTab({ ws, status, confirm, onNewWorktree }: { ws: string; status: GitStatus; confirm: (r: ConfirmRequest) => void; onNewWorktree: (branch: string) => void }) {
  const { data, error } = useGitData(ws, api.gitBranches);
  const [newOpen, setNewOpen] = useState(false);
  if (!data) return <Loading error={error} />;
  const local = data.filter((b) => !b.remote);
  const remote = data.filter((b) => b.remote);

  const remove = (b: Branch) =>
    confirm({
      title: `Excluir a branch ${b.name}?`,
      body: "Só branches já integradas são excluídas. Se houver commits não integrados, o FORGE pedirá uma confirmação extra.",
      confirmLabel: "Excluir",
      danger: true,
      run: async () => {
        try {
          await api.gitDeleteBranch(ws, b.name, false);
          toast.success(`Branch ${b.name} excluída`);
          window.dispatchEvent(new CustomEvent(GIT_CHANGED, { detail: { workspaceId: ws } }));
        } catch (e) {
          const msg = errorMessage(e);
          if (/not fully merged/i.test(msg)) {
            confirm({
              title: `A branch ${b.name} tem commits não integrados`,
              body: <span className="text-warning">Excluir mesmo assim apaga a referência para esses commits. Esta ação é difícil de desfazer.</span>,
              confirmLabel: "Excluir mesmo assim",
              danger: true,
              run: () => gitRun(ws, "Excluir branch", () => api.gitDeleteBranch(ws, b.name, true), { success: `Branch ${b.name} excluída` }),
            });
          } else toast.error("Excluir branch falhou", { description: msg });
        }
      },
    });

  const row = (b: Branch) => (
    <div key={b.name} className={cn("group flex items-center gap-2 rounded-sm px-2 py-1", b.current ? "bg-accent/10" : "hover:bg-elevated")}>
      <GitBranch className={cn("size-3.5 shrink-0", b.current ? "text-accent" : "text-subtle")} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5 text-[12.5px]">
          <span className={cn("truncate", b.current ? "font-medium text-fg" : "text-fg")}>{b.name}</span>
          {b.current && <Badge tone="accent">atual</Badge>}
          {b.track && <span className="shrink-0 text-[10.5px] text-subtle">{b.track}</span>}
        </div>
        <div className="truncate text-[11px] text-subtle">
          {b.shortOid} · {b.subject} · {relativeTime(b.time * 1000)}
        </div>
      </div>
      {!b.current && (
        <span className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
          <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => void gitRun(ws, "Trocar de branch", () => api.gitSwitch(ws, b.name, false), { worktreeChanged: true, success: `Branch ${b.name} ativada` })}>
            Ativar
          </Button>
          {!b.remote && (
            <>
              <Tooltip content={`Merge de ${b.name} em ${status.branch ?? "HEAD"}`}>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-6"
                  aria-label={`Merge de ${b.name}`}
                  onClick={() =>
                    confirm({
                      title: `Fazer merge de ${b.name} em ${status.branch ?? "HEAD"}?`,
                      body: "Conflitos, se houver, aparecem na aba Alterações. O merge pode ser abortado.",
                      confirmLabel: "Fazer merge",
                      run: () => gitRun(ws, "Merge", () => api.gitMerge(ws, b.name), { worktreeChanged: true, success: "Merge concluído" }),
                    })
                  }
                >
                  <GitMerge />
                </Button>
              </Tooltip>
              <Tooltip content="Criar worktree desta branch">
                <Button size="icon" variant="ghost" className="size-6" aria-label="Criar worktree" onClick={() => onNewWorktree(b.name)}>
                  <FolderOpen />
                </Button>
              </Tooltip>
              <Tooltip content="Excluir branch">
                <Button size="icon" variant="ghost" className="size-6" aria-label={`Excluir ${b.name}`} onClick={() => remove(b)}>
                  <Trash2 />
                </Button>
              </Tooltip>
            </>
          )}
        </span>
      )}
    </div>
  );

  return (
    <div className="h-full overflow-auto p-1.5">
      <div className="mb-1 flex items-center px-1">
        <span className="text-[10.5px] font-medium uppercase tracking-wider text-subtle">Locais</span>
        <Button size="sm" variant="ghost" className="ml-auto h-6" onClick={() => setNewOpen(true)}>
          <Plus /> Nova branch
        </Button>
      </div>
      {local.length === 0 ? <p className="px-2 py-1 text-[12px] text-subtle">Nenhuma branch (faça o primeiro commit).</p> : local.map(row)}
      {remote.length > 0 && (
        <>
          <div className="mt-3 mb-1 px-1 text-[10.5px] font-medium uppercase tracking-wider text-subtle">Remotas</div>
          {remote.map(row)}
        </>
      )}
      <NewBranchDialog ws={ws} status={status} open={newOpen} onClose={() => setNewOpen(false)} />
    </div>
  );
}

// ── History ───────────────────────────────────────────────────────────────

const loadLog = (ws: string) => api.gitLog(ws, 300);

export function HistoryTab({ ws, selected, onSelect }: { ws: string; selected: CommitTarget | null; onSelect: (t: CommitTarget) => void }) {
  const { data, error } = useGitData(ws, loadLog);
  const [open, setOpen] = useState<string | null>(null);
  const [files, setFiles] = useState<Record<string, GitCommitFile[] | "loading" | { error: string }>>({});

  const toggle = (c: GitCommit) => {
    if (open === c.sha) return setOpen(null);
    setOpen(c.sha);
    if (!files[c.sha]) {
      setFiles((f) => ({ ...f, [c.sha]: "loading" }));
      api
        .gitCommitFiles(ws, c.sha)
        .then((l) => setFiles((f) => ({ ...f, [c.sha]: l })))
        .catch((e) => setFiles((f) => ({ ...f, [c.sha]: { error: errorMessage(e) } })));
    }
  };

  if (!data) return <Loading error={error} />;
  if (data.length === 0) return <EmptyState title="Sem commits ainda" />;
  return (
    <div className="h-full overflow-auto py-1">
      {data.map((c) => {
        const list = files[c.sha];
        return (
          <div key={c.sha}>
            <button type="button" onClick={() => toggle(c)} className="flex w-full items-start gap-1.5 px-2 py-1.5 text-left hover:bg-elevated" title={`${c.sha}\n${c.author} <${c.email}>`}>
              <ChevronRight className={cn("mt-0.5 size-3 shrink-0 text-subtle transition-transform", open === c.sha && "rotate-90")} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="truncate text-[12.5px] text-fg">{c.subject}</span>
                  {c.refs.slice(0, 3).map((r) => (
                    <Badge key={r} tone={r.startsWith("HEAD") ? "accent" : "neutral"} className="shrink-0">
                      {r.replace("HEAD -> ", "")}
                    </Badge>
                  ))}
                </div>
                <div className="truncate text-[11px] text-subtle">
                  <span className="font-mono">{c.short}</span> · {c.author} · {relativeTime(c.time * 1000)}
                </div>
              </div>
            </button>
            {open === c.sha && (
              <div className="pb-1 pl-6">
                {list === "loading" || !list ? (
                  <Loader2 className="m-1 size-3.5 animate-spin text-subtle" />
                ) : "error" in list ? (
                  <p className="text-[11.5px] text-error">{list.error}</p>
                ) : (
                  list.map((f) => (
                    <button
                      key={f.path}
                      type="button"
                      onClick={() => onSelect({ kind: "commit", sha: c.sha, short: c.short, path: f.path, origPath: f.origPath, status: f.status })}
                      className={cn(
                        "flex h-6 w-full items-center gap-1.5 rounded-sm px-2 text-left text-[12px]",
                        selected?.sha === c.sha && selected.path === f.path ? "bg-accent/15 text-fg" : "text-muted hover:bg-elevated",
                      )}
                    >
                      <span className="w-3 font-mono text-[11px] font-semibold text-accent">{f.status}</span>
                      <span className="truncate">{f.path}</span>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ── Stash ─────────────────────────────────────────────────────────────────

export function StashTab({ ws, status, confirm }: { ws: string; status: GitStatus; confirm: (r: ConfirmRequest) => void }) {
  const { data, error } = useGitData(ws, api.gitStashList);
  const [message, setMessage] = useState("");
  const hasChanges = status.files.length > 0;
  return (
    <div className="flex h-full flex-col">
      <form
        className="flex shrink-0 gap-1.5 border-b border-border p-2"
        onSubmit={(e) => {
          e.preventDefault();
          void gitRun(ws, "Stash", () => api.gitStashPush(ws, message.trim() || null), { worktreeChanged: true, success: "Alterações guardadas no stash" }).then(() => setMessage(""));
        }}
      >
        <Input className="h-7 text-[12px]" value={message} onChange={(e) => setMessage(e.target.value)} placeholder="Mensagem (opcional)" disabled={!hasChanges} />
        <Button size="sm" type="submit" className="h-7 shrink-0" disabled={!hasChanges}>
          <Archive /> Guardar
        </Button>
      </form>
      <div className="min-h-0 flex-1 overflow-auto p-1.5">
        {!data ? (
          <Loading error={error} />
        ) : data.length === 0 ? (
          <EmptyState title="Stash vazio" description="Guarde alterações temporariamente sem commitar (inclui arquivos novos)." />
        ) : (
          data.map((s: GitStash) => (
            <div key={s.index} className="group flex items-center gap-2 rounded-sm px-2 py-1 hover:bg-elevated">
              <Archive className="size-3.5 shrink-0 text-subtle" />
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12.5px] text-fg">{s.message}</div>
                <div className="text-[11px] text-subtle">
                  stash@{"{"}
                  {s.index}
                  {"}"} · {relativeTime(s.time * 1000)}
                </div>
              </div>
              <span className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
                <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => void gitRun(ws, "Aplicar stash", () => api.gitStashAction(ws, "apply", s.index), { worktreeChanged: true, success: "Stash aplicado" })}>
                  Aplicar
                </Button>
                <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => void gitRun(ws, "Pop stash", () => api.gitStashAction(ws, "pop", s.index), { worktreeChanged: true, success: "Stash aplicado e removido" })}>
                  Pop
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-6"
                  aria-label="Excluir stash"
                  onClick={() =>
                    confirm({
                      title: "Excluir este stash?",
                      description: s.message,
                      body: "As alterações guardadas nele serão perdidas.",
                      confirmLabel: "Excluir",
                      danger: true,
                      run: () => gitRun(ws, "Excluir stash", () => api.gitStashAction(ws, "drop", s.index), { success: "Stash excluído" }),
                    })
                  }
                >
                  <Trash2 />
                </Button>
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

// ── Worktrees ─────────────────────────────────────────────────────────────

export function NewWorktreeDialog({ ws, status, branch, open, onClose }: { ws: string; status: GitStatus; branch?: string | null; open: boolean; onClose: () => void }) {
  const [mode, setMode] = useState<"new" | "existing">("new");
  const [name, setName] = useState("");
  const [base, setBase] = useState("");
  const [register, setRegister] = useState(true);
  useEffect(() => {
    if (!open) return;
    setMode(branch ? "existing" : "new");
    setName(branch ?? "");
    setBase(status.branch ?? "HEAD");
    setRegister(true);
  }, [open, branch, status.branch]);

  const submit = async () => {
    const n = name.trim();
    if (!n) return;
    onClose();
    const res = await gitRun(ws, "Criar worktree", () => api.gitWorktreeAdd(ws, n, mode === "new", mode === "new" ? base || null : null, register), {
      success: (r) => `Worktree criada em ${prettyPath(r.worktree.path)}`,
    });
    if (res?.workspace) {
      await useWorkspaces.getState().refresh();
      await openWorkspace(res.workspace.id);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent title="Nova worktree" description="Uma cópia de trabalho isolada do repositório, em outra pasta, com sua própria branch.">
        <form
          className="space-y-4 px-5 py-4"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <div className="grid grid-cols-2 gap-2">
            {(["new", "existing"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                className={cn("rounded-md border px-3 py-2 text-left text-[12px]", mode === m ? "border-accent/60 bg-accent/10 text-fg" : "border-border text-muted hover:border-border-strong")}
              >
                {m === "new" ? "Nova branch" : "Branch existente"}
              </button>
            ))}
          </div>
          <div>
            <Label htmlFor="wt-name">{mode === "new" ? "Nome da nova branch" : "Branch"}</Label>
            <Input id="wt-name" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={mode === "new" ? "task/minha-tarefa" : "feat/x"} />
          </div>
          {mode === "new" && (
            <div>
              <Label htmlFor="wt-base">A partir de</Label>
              <Input id="wt-base" value={base} onChange={(e) => setBase(e.target.value)} />
            </div>
          )}
          <label className="flex items-center gap-2 text-[12.5px] text-muted">
            <input type="checkbox" checked={register} onChange={(e) => setRegister(e.target.checked)} className="accent-[#579bff]" />
            Abrir como workspace no FORGE
          </label>
          <p className="text-[11.5px] leading-relaxed text-subtle">A pasta é criada na área gerenciada pelo FORGE (dados do app), fora do seu repositório.</p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancelar
            </Button>
            <Button type="submit" variant="primary" disabled={!name.trim()}>
              Criar worktree
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function WorktreesTab({ ws, status, confirm, onNew }: { ws: string; status: GitStatus; confirm: (r: ConfirmRequest) => void; onNew: () => void }) {
  const { data, error } = useGitData(ws, api.gitWorktrees);
  const workspaces = useWorkspaces((s) => s.items);

  const open = async (w: GitWorktree) => {
    try {
      const existing = workspaces.find((x) => x.path === w.path);
      const target = existing ?? (await useWorkspaces.getState().add(w.path));
      await openWorkspace(target.id);
    } catch (e) {
      toast.error("Não foi possível abrir", { description: errorMessage(e) });
    }
  };

  const remove = (w: GitWorktree, force = false) =>
    confirm({
      title: force ? "Remover worktree com alterações?" : "Remover worktree?",
      description: w.path,
      body: force ? (
        <span className="text-warning">A worktree tem alterações não commitadas ou arquivos novos. Elas serão perdidas.</span>
      ) : (
        "A pasta da worktree é removida (a branch continua existindo). Se estiver registrada como workspace, ela sai da lista e seus terminais são encerrados."
      ),
      confirmLabel: force ? "Remover e perder alterações" : "Remover",
      danger: true,
      run: async () => {
        try {
          await api.gitWorktreeRemove(ws, w.path, force);
          toast.success("Worktree removida");
          await useWorkspaces.getState().refresh();
          window.dispatchEvent(new CustomEvent(GIT_CHANGED, { detail: { workspaceId: ws } }));
        } catch (e) {
          const msg = errorMessage(e);
          if (!force && /contains modified or untracked files|is dirty|use --force/i.test(msg)) remove(w, true);
          else toast.error("Remover worktree falhou", { description: msg });
        }
      },
    });

  return (
    <div className="h-full overflow-auto p-1.5">
      <div className="mb-1 flex items-center px-1">
        <span className="text-[10.5px] font-medium uppercase tracking-wider text-subtle">Worktrees</span>
        <Button size="sm" variant="ghost" className="ml-auto h-6" disabled={!status.hasCommits} onClick={onNew}>
          <Plus /> Nova worktree
        </Button>
      </div>
      {!status.hasCommits && <p className="px-2 py-1 text-[11.5px] text-subtle">Faça o primeiro commit para criar worktrees.</p>}
      {!data ? (
        <Loading error={error} />
      ) : (
        data.map((w) => {
          const registered = workspaces.find((x) => x.path === w.path);
          return (
            <div key={w.path} className="group flex items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-elevated">
              <FolderOpen className={cn("size-3.5 shrink-0", w.main ? "text-accent" : "text-subtle")} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-[12.5px] text-fg">
                  <span className="truncate">{w.branch ?? (w.detached ? `HEAD ${w.head?.slice(0, 7) ?? ""}` : "—")}</span>
                  {w.main && <Badge tone="accent">principal</Badge>}
                  {w.locked && <Badge tone="warning">bloqueada</Badge>}
                  {w.prunable && <Badge tone="error">ausente</Badge>}
                  {registered && !w.main && <Badge>workspace</Badge>}
                </div>
                <div className="truncate font-mono text-[11px] text-subtle" title={w.path}>
                  {prettyPath(w.path)}
                </div>
              </div>
              {!w.main && (
                <span className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
                  <Button size="sm" variant="ghost" className="h-6 px-2" disabled={w.prunable} onClick={() => void open(w)}>
                    Abrir
                  </Button>
                  <Button size="icon" variant="ghost" className="size-6" aria-label="Remover worktree" onClick={() => remove(w)}>
                    <Trash2 />
                  </Button>
                </span>
              )}
            </div>
          );
        })
      )}
    </div>
  );
}
