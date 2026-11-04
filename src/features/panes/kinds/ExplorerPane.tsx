import { useCallback, useEffect, useState } from "react";
import {
  ChevronRight,
  ChevronsDownUp,
  ClipboardCopy,
  File,
  FilePlus2,
  Folder,
  FolderOpen,
  FolderPlus,
  Link2,
  Loader2,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Dialog,
  DialogContent,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Input,
  Label,
  Tooltip,
} from "@/components/ui";
import { closeTabsUnder, openFileInEditor, readEditorState, renameTabs } from "@/features/editor/actions";
import { invalidateFileList } from "@/features/editor/QuickOpen";
import { fileKey, useEditorStatus } from "@/features/editor/status";
import { api, errorMessage, type DirEntry } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { findPane, panes, setPaneState } from "../layout";
import type { PaneProps } from "../registry";
import { useLayout } from "../store";

type ExplorerState = { expanded: string[]; selected: string | null };

function readState(raw: Record<string, unknown> | undefined): ExplorerState {
  const expanded = Array.isArray(raw?.expanded) ? (raw.expanded as unknown[]).filter((x): x is string => typeof x === "string") : [];
  const selected = typeof raw?.selected === "string" ? raw.selected : null;
  return { expanded, selected };
}

type DirCache = Record<string, { entries: DirEntry[]; truncated: boolean } | { error: string } | "loading">;

const parentOf = (rel: string) => (rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "");
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);
const under = (p: string, prefix: string) => p === prefix || p.startsWith(prefix + "/");

type Prompt =
  | { kind: "new-file" | "new-folder"; dir: string }
  | { kind: "rename"; entry: DirEntry }
  | { kind: "trash"; entry: DirEntry };

/** File currently shown by the active session's focused/first editor (for highlighting). */
function useActiveEditorFile(): string | null {
  return useLayout((s) => {
    const doc = s.doc;
    if (!doc) return null;
    const editors = panes(doc.root).filter((p) => p.kind === "editor");
    const ed = editors.find((p) => p.id === doc.focusedId) ?? editors[0];
    return ed ? readEditorState(ed.state).active : null;
  });
}

export function ExplorerPane({ pane, workspaceId }: PaneProps) {
  const apply = useLayout((s) => s.apply);
  const state = readState(pane.state);
  const expanded = new Set(state.expanded);
  const activeFile = useActiveEditorFile();
  const dirtyMap = useEditorStatus((s) => s.dirty);
  const [dirs, setDirs] = useState<DirCache>({});
  const [prompt, setPrompt] = useState<Prompt | null>(null);

  const update = (next: Partial<ExplorerState>) =>
    apply((d) => {
      const p = findPane(d, pane.id);
      return p ? setPaneState(d, pane.id, { ...readState(p.state), ...next }) : d;
    });

  const loadDir = useCallback(
    async (rel: string) => {
      setDirs((d) => ({ ...d, [rel]: d[rel] && d[rel] !== "loading" ? d[rel] : "loading" }));
      try {
        const l = await api.fsListDir(workspaceId, rel);
        setDirs((d) => ({ ...d, [rel]: { entries: l.entries, truncated: l.truncated } }));
      } catch (e) {
        setDirs((d) => ({ ...d, [rel]: { error: errorMessage(e) } }));
      }
    },
    [workspaceId],
  );

  useEffect(() => {
    setDirs({});
    void loadDir("");
    for (const rel of state.expanded) void loadDir(rel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId, loadDir]);

  const refreshAll = () => {
    void loadDir("");
    for (const rel of expanded) void loadDir(rel);
  };

  const toggleDir = (rel: string) => {
    const next = new Set(expanded);
    if (next.has(rel)) next.delete(rel);
    else {
      next.add(rel);
      void loadDir(rel);
    }
    update({ expanded: [...next], selected: rel });
  };

  const selectedDir = (() => {
    const sel = state.selected;
    if (!sel) return "";
    const entry = Object.values(dirs)
      .flatMap((d) => (typeof d === "object" && "entries" in d ? d.entries : []))
      .find((e) => e.relPath === sel);
    return entry?.isDir ? sel : parentOf(sel);
  })();

  // ── File operations ──────────────────────────────────────────────────────
  const afterChange = (...dirsToReload: string[]) => {
    invalidateFileList(workspaceId);
    window.dispatchEvent(new CustomEvent("forge:files-changed", { detail: { workspaceId } }));
    for (const d of new Set(dirsToReload)) void loadDir(d);
  };

  const create = async (dir: string, name: string, isDir: boolean) => {
    const rel = join(dir, name);
    await api.fsCreate(workspaceId, rel, isDir);
    if (dir && !expanded.has(dir)) update({ expanded: [...expanded, dir], selected: rel });
    else update({ selected: rel });
    afterChange(dir);
    if (!isDir) openFileInEditor(rel);
  };

  const rename = async (entry: DirEntry, name: string) => {
    const dir = parentOf(entry.relPath);
    const to = join(dir, name);
    if (to === entry.relPath) return;
    await api.fsRename(workspaceId, entry.relPath, to);
    renameTabs(entry.relPath, to);
    const models = await import("@/features/editor/models").catch(() => null);
    models?.renamePaths(workspaceId, entry.relPath, to);
    update({
      expanded: state.expanded.map((e) => (under(e, entry.relPath) ? to + e.slice(entry.relPath.length) : e)),
      selected: to,
    });
    afterChange(dir);
  };

  const trash = async (entry: DirEntry) => {
    await api.fsTrash(workspaceId, entry.relPath);
    closeTabsUnder(entry.relPath);
    const models = await import("@/features/editor/models").catch(() => null);
    const status = useEditorStatus.getState();
    for (const key of Object.keys({ ...status.dirty, ...status.conflicts })) {
      const rel = key.split("\u0000")[1] ?? "";
      if (key.startsWith(workspaceId + "\u0000") && under(rel, entry.relPath)) models?.release(workspaceId, rel);
    }
    update({ expanded: state.expanded.filter((e) => !under(e, entry.relPath)), selected: null });
    afterChange(parentOf(entry.relPath));
    toast.success("Movido para a lixeira", { description: entry.relPath });
  };

  // ── Render ───────────────────────────────────────────────────────────────
  const renderDir = (rel: string, depth: number): React.ReactNode => {
    const entry = dirs[rel];
    const pad = depth * 12;
    if (!entry || entry === "loading")
      return (
        <div className="flex h-6 items-center gap-1.5 text-subtle" style={{ paddingLeft: pad + 22 }}>
          <Loader2 className="size-3 animate-spin" /> <span className="text-[11.5px]">Carregando…</span>
        </div>
      );
    if ("error" in entry)
      return (
        <div className="py-1 text-[11.5px] text-error" style={{ paddingLeft: pad + 22 }}>
          {entry.error}
        </div>
      );
    if (entry.entries.length === 0 && depth > 0)
      return (
        <div className="py-0.5 text-[11.5px] text-subtle italic" style={{ paddingLeft: pad + 22 }}>
          vazio
        </div>
      );
    return (
      <>
        {entry.entries.map((e) => {
          const isOpen = expanded.has(e.relPath);
          const isSel = state.selected === e.relPath;
          const isActive = !e.isDir && activeFile === e.relPath;
          return (
            <div key={e.relPath}>
              <div
                className={cn(
                  "group flex h-6 items-center rounded-sm pr-1",
                  isSel ? "bg-accent/15" : isActive ? "bg-elevated" : "hover:bg-elevated",
                )}
              >
                <button
                  type="button"
                  className={cn("flex h-full min-w-0 flex-1 items-center gap-1.5 text-left text-[12.5px]", isSel || isActive ? "text-fg" : "text-muted")}
                  style={{ paddingLeft: pad + 6 }}
                  onClick={() => {
                    if (e.isDir) toggleDir(e.relPath);
                    else {
                      update({ selected: e.relPath });
                      openFileInEditor(e.relPath);
                    }
                  }}
                  title={e.relPath}
                >
                  {e.isDir ? (
                    <ChevronRight className={cn("size-3 shrink-0 text-subtle transition-transform", isOpen && "rotate-90")} />
                  ) : (
                    <span className="w-3 shrink-0" />
                  )}
                  {e.isDir ? (
                    isOpen ? <FolderOpen className="size-3.5 shrink-0 text-accent-2" /> : <Folder className="size-3.5 shrink-0 text-accent-2" />
                  ) : (
                    <File className="size-3.5 shrink-0 text-subtle" />
                  )}
                  <span className="truncate">{e.name}</span>
                  {e.isSymlink && <Link2 className="size-3 shrink-0 text-subtle" />}
                  {!e.isDir && dirtyMap[fileKey(workspaceId, e.relPath)] && (
                    <span className="ml-auto size-1.5 shrink-0 rounded-full bg-warning" title="Alterações não salvas" />
                  )}
                </button>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <button
                      type="button"
                      aria-label={`Ações de ${e.name}`}
                      className="flex size-5 shrink-0 items-center justify-center rounded text-subtle opacity-0 group-hover:opacity-100 hover:bg-surface hover:text-fg data-[state=open]:opacity-100"
                    >
                      <MoreHorizontal className="size-3.5" />
                    </button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="start" side="right">
                    {e.isDir && (
                      <>
                        <DropdownMenuItem onSelect={() => setPrompt({ kind: "new-file", dir: e.relPath })}>
                          <FilePlus2 /> Novo arquivo aqui
                        </DropdownMenuItem>
                        <DropdownMenuItem onSelect={() => setPrompt({ kind: "new-folder", dir: e.relPath })}>
                          <FolderPlus /> Nova pasta aqui
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                      </>
                    )}
                    <DropdownMenuItem onSelect={() => setPrompt({ kind: "rename", entry: e })}>
                      <Pencil /> Renomear
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      onSelect={() =>
                        void navigator.clipboard
                          .writeText(e.relPath)
                          .then(() => toast.success("Caminho copiado", { description: e.relPath }))
                          .catch(() => toast.error("Não foi possível copiar"))
                      }
                    >
                      <ClipboardCopy /> Copiar caminho relativo
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem className="text-error" onSelect={() => setPrompt({ kind: "trash", entry: e })}>
                      <Trash2 /> Mover para a lixeira
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
              {e.isDir && isOpen && renderDir(e.relPath, depth + 1)}
            </div>
          );
        })}
        {entry.truncated && (
          <div className="py-1 text-[11px] text-warning" style={{ paddingLeft: pad + 22 }}>
            Lista truncada (diretório muito grande)
          </div>
        )}
      </>
    );
  };

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-7 shrink-0 items-center gap-0.5 px-2">
        <span className="mr-auto text-[10.5px] font-medium uppercase tracking-wider text-subtle">Arquivos</span>
        <Tooltip content="Novo arquivo">
          <Button variant="ghost" size="icon" className="size-6" aria-label="Novo arquivo" onClick={() => setPrompt({ kind: "new-file", dir: selectedDir })}>
            <FilePlus2 />
          </Button>
        </Tooltip>
        <Tooltip content="Nova pasta">
          <Button variant="ghost" size="icon" className="size-6" aria-label="Nova pasta" onClick={() => setPrompt({ kind: "new-folder", dir: selectedDir })}>
            <FolderPlus />
          </Button>
        </Tooltip>
        <Tooltip content="Recolher pastas">
          <Button variant="ghost" size="icon" className="size-6" aria-label="Recolher pastas" onClick={() => update({ expanded: [] })}>
            <ChevronsDownUp />
          </Button>
        </Tooltip>
        <Tooltip content="Atualizar">
          <Button variant="ghost" size="icon" className="size-6" aria-label="Atualizar árvore" onClick={refreshAll}>
            <RefreshCw />
          </Button>
        </Tooltip>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-1 pb-2">{renderDir("", 0)}</div>
      <FileOpDialog
        prompt={prompt}
        onClose={() => setPrompt(null)}
        onCreate={create}
        onRename={rename}
        onTrash={trash}
        dirtyUnder={(rel) => Object.keys(useEditorStatus.getState().dirty).some((k) => k.startsWith(workspaceId + "\u0000") && under(k.split("\u0000")[1] ?? "", rel))}
      />
    </div>
  );
}

function validName(name: string): string | null {
  const n = name.trim();
  if (!n) return "Informe um nome.";
  if (n.includes("/") || n.includes("\\")) return "O nome não pode conter barras.";
  if (n === "." || n === "..") return "Nome inválido.";
  if (n.length > 255) return "Nome muito longo.";
  return null;
}

function FileOpDialog({
  prompt,
  onClose,
  onCreate,
  onRename,
  onTrash,
  dirtyUnder,
}: {
  prompt: Prompt | null;
  onClose: () => void;
  onCreate: (dir: string, name: string, isDir: boolean) => Promise<void>;
  onRename: (entry: DirEntry, name: string) => Promise<void>;
  onTrash: (entry: DirEntry) => Promise<void>;
  dirtyUnder: (rel: string) => boolean;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setName(prompt?.kind === "rename" ? prompt.entry.name : "");
    setBusy(false);
  }, [prompt]);

  if (!prompt) return null;
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
      onClose();
    } catch (e) {
      toast.error("Operação falhou", { description: errorMessage(e) });
      setBusy(false);
    }
  };

  if (prompt.kind === "trash") {
    const hasDirty = dirtyUnder(prompt.entry.relPath);
    return (
      <Dialog open onOpenChange={(o) => !o && onClose()}>
        <DialogContent title="Mover para a lixeira?" description={prompt.entry.relPath}>
          <div className="px-5 py-4 text-[13px] text-muted">
            {prompt.entry.isDir ? "A pasta e todo o seu conteúdo serão movidos" : "O arquivo será movido"} para a lixeira do sistema. Você pode recuperá-lo de lá.
            {hasDirty && <p className="mt-2 text-warning">Há alterações não salvas neste caminho; elas serão descartadas.</p>}
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={onClose}>
                Cancelar
              </Button>
              <Button variant="danger" disabled={busy} onClick={() => void run(() => onTrash(prompt.entry))}>
                <Trash2 /> Mover para a lixeira
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  const error = validName(name);
  const title = prompt.kind === "rename" ? "Renomear" : prompt.kind === "new-file" ? "Novo arquivo" : "Nova pasta";
  const where = prompt.kind === "rename" ? prompt.entry.relPath : prompt.dir ? `em ${prompt.dir}/` : "na raiz do workspace";
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={title} description={where}>
        <form
          className="space-y-4 px-5 py-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (error || busy) return;
            const n = name.trim();
            if (prompt.kind === "rename") void run(() => onRename(prompt.entry, n));
            else void run(() => onCreate(prompt.dir, n, prompt.kind === "new-folder"));
          }}
        >
          <div>
            <Label htmlFor="fileop-name">Nome</Label>
            <Input
              id="fileop-name"
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onFocus={(e) => {
                // Select the base name (without extension) when renaming files.
                if (prompt.kind === "rename" && !prompt.entry.isDir) {
                  const dot = e.target.value.lastIndexOf(".");
                  e.target.setSelectionRange(0, dot > 0 ? dot : e.target.value.length);
                }
              }}
            />
            {name && error && <p className="mt-1 text-[11.5px] text-error">{error}</p>}
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancelar
            </Button>
            <Button type="submit" variant="primary" disabled={!!error || busy}>
              {prompt.kind === "rename" ? "Renomear" : "Criar"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
