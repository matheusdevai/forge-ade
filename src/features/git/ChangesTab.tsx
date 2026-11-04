import { useState } from "react";
import { FileCode2, Minus, Plus, RotateCcw, TriangleAlert } from "lucide-react";
import { Button, EmptyState, Kbd, Tooltip } from "@/components/ui";
import { openFileInEditor } from "@/features/editor/actions";
import { api, type GitFileStatus, type GitStatus } from "@/lib/ipc";
import { cn, modKey } from "@/lib/utils";
import type { ConfirmRequest } from "./Confirm";
import { gitRun, toWorkspacePath } from "./gitState";

export type ChangeArea = "staged" | "unstaged" | "untracked" | "conflict";
export type ChangeTarget = { kind: "change"; path: string; origPath: string | null; area: ChangeArea };

const LETTER_TONE: Record<string, string> = {
  M: "text-warning",
  A: "text-success",
  "?": "text-success",
  D: "text-error",
  R: "text-accent",
  C: "text-accent",
  T: "text-warning",
  U: "text-error",
};

const LETTER_LABEL: Record<string, string> = {
  M: "Modificado",
  A: "Adicionado",
  "?": "Novo (não rastreado)",
  D: "Removido",
  R: "Renomeado",
  C: "Copiado",
  T: "Tipo alterado",
  U: "Conflito",
};

function FileRow({
  file,
  letter,
  selected,
  onSelect,
  actions,
  workspaceRel,
}: {
  file: GitFileStatus;
  letter: string;
  selected: boolean;
  onSelect: () => void;
  actions: React.ReactNode;
  workspaceRel: string | null;
}) {
  const name = file.path.slice(file.path.lastIndexOf("/") + 1);
  const dir = file.path.includes("/") ? file.path.slice(0, file.path.lastIndexOf("/")) : "";
  return (
    <div className={cn("group flex h-6 items-center gap-1.5 rounded-sm pr-1 pl-2", selected ? "bg-accent/15" : "hover:bg-elevated")}>
      <button type="button" onClick={onSelect} className="flex min-w-0 flex-1 items-center gap-1.5 text-left" title={file.origPath ? `${file.origPath} → ${file.path}` : file.path}>
        <span className={cn("truncate text-[12.5px]", letter === "D" ? "text-muted line-through" : "text-fg")}>{name}</span>
        <span className="truncate text-[11px] text-subtle">{dir}</span>
      </button>
      <span className="flex shrink-0 items-center opacity-0 group-hover:opacity-100">
        {workspaceRel && letter !== "D" && (
          <Tooltip content="Abrir arquivo">
            <button type="button" aria-label="Abrir arquivo" className="flex size-5 items-center justify-center rounded text-subtle hover:bg-surface hover:text-fg" onClick={() => openFileInEditor(workspaceRel)}>
              <FileCode2 className="size-3.5" />
            </button>
          </Tooltip>
        )}
        {actions}
      </span>
      <Tooltip content={LETTER_LABEL[letter] ?? letter}>
        <span className={cn("w-3 shrink-0 text-center font-mono text-[11px] font-semibold", LETTER_TONE[letter] ?? "text-muted")}>{letter === "?" ? "U" : letter}</span>
      </Tooltip>
    </div>
  );
}

function IconAction({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <Tooltip content={label}>
      <button type="button" aria-label={label} onClick={onClick} className="flex size-5 items-center justify-center rounded text-subtle hover:bg-surface hover:text-fg">
        {children}
      </button>
    </Tooltip>
  );
}

function Section({ title, count, actions, children }: { title: string; count: number; actions?: React.ReactNode; children: React.ReactNode }) {
  if (count === 0) return null;
  return (
    <div className="mb-2">
      <div className="group flex h-6 items-center px-2">
        <span className="text-[10.5px] font-medium uppercase tracking-wider text-subtle">{title}</span>
        <span className="ml-1.5 rounded bg-elevated px-1 text-[10px] text-muted">{count}</span>
        <span className="ml-auto flex items-center opacity-0 group-hover:opacity-100">{actions}</span>
      </div>
      {children}
    </div>
  );
}

export function ChangesTab({
  ws,
  status,
  selected,
  onSelect,
  confirm,
}: {
  ws: string;
  status: GitStatus;
  selected: ChangeTarget | null;
  onSelect: (t: ChangeTarget | null) => void;
  confirm: (r: ConfirmRequest) => void;
}) {
  const [message, setMessage] = useState("");
  const [amend, setAmend] = useState(false);

  const conflicts = status.files.filter((f) => f.conflicted);
  const staged = status.files.filter((f) => !f.conflicted && !f.untracked && f.index !== ".");
  const unstaged = status.files.filter((f) => !f.conflicted && (f.untracked || f.worktree !== "."));

  const isSel = (f: GitFileStatus, area: ChangeArea) => selected?.path === f.path && selected.area === area;
  const select = (f: GitFileStatus, area: ChangeArea) => onSelect({ kind: "change", path: f.path, origPath: f.origPath, area });

  const commit = async () => {
    const msg = message.trim();
    if (!msg || (!staged.length && !amend)) return;
    const sha = await gitRun(ws, "Commit", () => api.gitCommit(ws, msg, amend), { success: (s) => `Commit ${s.slice(0, 7)} criado` });
    if (sha) {
      setMessage("");
      setAmend(false);
      onSelect(null);
    }
  };

  const discard = (paths: string[], label: string) =>
    confirm({
      title: "Descartar alterações?",
      description: label,
      body: (
        <>
          As alterações locais serão perdidas. Arquivos novos (não rastreados) vão para a lixeira do sistema.
        </>
      ),
      confirmLabel: "Descartar",
      danger: true,
      run: () => gitRun(ws, "Descartar", () => api.gitDiscard(ws, paths), { worktreeChanged: true }).then(() => onSelect(null)),
    });

  return (
    <div className="flex h-full min-h-0 flex-col">
      {status.merging && (
        <div className="m-2 rounded-md border border-warning/30 bg-warning/10 p-2 text-[12px] text-warning">
          <div className="flex items-center gap-1.5 font-medium">
            <TriangleAlert className="size-3.5" /> Merge em andamento
          </div>
          <p className="mt-0.5 text-[11.5px] text-warning/80">Resolva os conflitos, faça stage dos arquivos e conclua com um commit.</p>
          <Button
            size="sm"
            variant="ghost"
            className="mt-1.5 h-6"
            onClick={() =>
              confirm({
                title: "Abortar merge?",
                body: "O repositório volta ao estado anterior ao merge.",
                confirmLabel: "Abortar merge",
                danger: true,
                run: () => gitRun(ws, "Abortar merge", () => api.gitMergeAbort(ws), { worktreeChanged: true, success: "Merge abortado" }),
              })
            }
          >
            Abortar merge
          </Button>
        </div>
      )}

      <div className="shrink-0 space-y-1.5 border-b border-border p-2">
        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void commit();
            }
          }}
          rows={3}
          placeholder={status.merging ? "Mensagem do commit de merge" : "Mensagem do commit"}
          aria-label="Mensagem do commit"
          className="w-full resize-none rounded-md border border-border bg-bg px-2.5 py-1.5 text-[12.5px] leading-relaxed text-fg placeholder:text-subtle focus:border-accent/60 focus:outline-none"
        />
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1.5 text-[11.5px] text-muted">
            <input type="checkbox" checked={amend} disabled={!status.hasCommits} onChange={(e) => setAmend(e.target.checked)} className="accent-[#579bff]" />
            Amend
          </label>
          <Button size="sm" variant="primary" className="ml-auto h-7" disabled={!message.trim() || (!staged.length && !amend)} onClick={() => void commit()}>
            Commit{staged.length ? ` (${staged.length})` : ""} <Kbd className="border-transparent bg-black/20 text-[#05080d]">{modKey}↵</Kbd>
          </Button>
        </div>
        {!staged.length && unstaged.length > 0 && !amend && <p className="text-[11px] text-subtle">Faça stage das alterações para commitar.</p>}
      </div>

      <div className="min-h-0 flex-1 overflow-auto py-1.5">
        {status.files.length === 0 && <EmptyState title="Nenhuma alteração" description="A árvore de trabalho está limpa." />}
        {status.truncated && <p className="px-2 pb-1 text-[11px] text-warning">Lista truncada em 5.000 arquivos.</p>}

        <Section title="Conflitos" count={conflicts.length}>
          {conflicts.map((f) => (
            <FileRow
              key={f.path}
              file={f}
              letter="U"
              selected={isSel(f, "conflict")}
              onSelect={() => select(f, "conflict")}
              workspaceRel={toWorkspacePath(status, f.path)}
              actions={
                <IconAction label="Marcar como resolvido (stage)" onClick={() => void gitRun(ws, "Stage", () => api.gitStage(ws, [f.path]))}>
                  <Plus className="size-3.5" />
                </IconAction>
              }
            />
          ))}
        </Section>

        <Section
          title="Staged"
          count={staged.length}
          actions={
            <IconAction label="Unstage tudo" onClick={() => void gitRun(ws, "Unstage", () => api.gitUnstage(ws, staged.map((f) => f.path)))}>
              <Minus className="size-3.5" />
            </IconAction>
          }
        >
          {staged.map((f) => (
            <FileRow
              key={f.path}
              file={f}
              letter={f.index}
              selected={isSel(f, "staged")}
              onSelect={() => select(f, "staged")}
              workspaceRel={toWorkspacePath(status, f.path)}
              actions={
                <IconAction label="Unstage" onClick={() => void gitRun(ws, "Unstage", () => api.gitUnstage(ws, [f.path]))}>
                  <Minus className="size-3.5" />
                </IconAction>
              }
            />
          ))}
        </Section>

        <Section
          title="Alterações"
          count={unstaged.length}
          actions={
            <>
              <IconAction label="Descartar tudo" onClick={() => discard(unstaged.map((f) => f.path), `${unstaged.length} arquivo(s)`)}>
                <RotateCcw className="size-3.5" />
              </IconAction>
              <IconAction label="Stage tudo" onClick={() => void gitRun(ws, "Stage", () => api.gitStage(ws, unstaged.map((f) => f.path)))}>
                <Plus className="size-3.5" />
              </IconAction>
            </>
          }
        >
          {unstaged.map((f) => {
            const area: ChangeArea = f.untracked ? "untracked" : "unstaged";
            return (
              <FileRow
                key={f.path}
                file={f}
                letter={f.untracked ? "?" : f.worktree}
                selected={isSel(f, area)}
                onSelect={() => select(f, area)}
                workspaceRel={toWorkspacePath(status, f.path)}
                actions={
                  <>
                    <IconAction label="Descartar alterações" onClick={() => discard([f.path], f.path)}>
                      <RotateCcw className="size-3.5" />
                    </IconAction>
                    <IconAction label="Stage" onClick={() => void gitRun(ws, "Stage", () => api.gitStage(ws, [f.path]))}>
                      <Plus className="size-3.5" />
                    </IconAction>
                  </>
                }
              />
            );
          })}
        </Section>
      </div>
    </div>
  );
}
