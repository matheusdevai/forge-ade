import { useCallback, useEffect, useRef, useState } from "react";
import { FileCode2, FileWarning, Lock, TriangleAlert, X } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, DialogContent, EmptyState, Kbd } from "@/components/ui";
import { findPane, focusPane, setPaneState } from "@/features/panes/layout";
import type { PaneProps } from "@/features/panes/registry";
import { useLayout } from "@/features/panes/store";
import { errorMessage, IpcError } from "@/lib/ipc";
import { cn, modKey } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { markEditorFocused, readEditorState, referencedFiles, subscribeReveal, takeReveal, type EditorState, type Reveal } from "./actions";
import { acquire, BinaryFileError, checkExternalChange, release, releaseUnreferenced, reloadFromDisk, save } from "./models";
import { monaco } from "./monaco";
import { fileKey, useEditorStatus } from "./status";

const viewStates = new Map<string, monaco.editor.ICodeEditorViewState | null>();
const FONT = '"Geist Mono Variable", ui-monospace, "SF Mono", Menlo, Consolas, monospace';

const basename = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const dirname = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

function languageName(id: string) {
  return monaco.languages.getLanguages().find((l) => l.id === id)?.aliases?.[0] ?? id;
}

type Meta = { language: string; eol: "LF" | "CRLF"; readOnly: string | null };

export function EditorPane({ pane, workspaceId }: PaneProps) {
  const state = readEditorState(pane.state);
  const apply = useLayout((s) => s.apply);
  const focused = useLayout((s) => s.doc?.focusedId === pane.id);
  const openQuickOpen = useApp((s) => s.setQuickOpen);
  const dirty = useEditorStatus((s) => s.dirty);
  const conflicts = useEditorStatus((s) => s.conflicts);

  const hostRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const activeRef = useRef<string | null>(null);
  const wsRef = useRef(workspaceId);
  wsRef.current = workspaceId;

  const [fileError, setFileError] = useState<{ rel: string; message: string; binary: boolean } | null>(null);
  const [meta, setMeta] = useState<Meta | null>(null);
  const [cursor, setCursor] = useState({ line: 1, col: 1, selected: 0 });
  const [confirmClose, setConfirmClose] = useState<string | null>(null);

  const updateState = useCallback(
    (fn: (s: EditorState) => EditorState) =>
      apply((d) => {
        const p = findPane(d, pane.id);
        return p ? setPaneState(d, pane.id, { ...(p.state ?? {}), ...fn(readEditorState(p.state)) }) : d;
      }),
    [apply, pane.id],
  );

  const saveFile = useCallback(async (rel: string, force = false) => {
    const ws = wsRef.current;
    try {
      await save(ws, rel, force);
    } catch (e) {
      if (e instanceof IpcError && e.kind === "conflict") {
        useEditorStatus.getState().setConflict(fileKey(ws, rel), true);
        toast.warning("O arquivo foi alterado fora do FORGE", { description: "Escolha recarregar ou sobrescrever na barra do editor." });
      } else {
        toast.error("Falha ao salvar", { description: errorMessage(e) });
      }
      throw e;
    }
  }, []);

  const applyReveal = useCallback((ed: monaco.editor.IStandaloneCodeEditor, r: Reveal) => {
    const range = new monaco.Range(r.line, r.column, r.line, r.column + (r.length ?? 0));
    ed.setSelection(range);
    ed.revealRangeInCenterIfOutsideViewport(range, monaco.editor.ScrollType.Smooth);
    ed.focus();
  }, []);

  // Create the Monaco instance once per mount.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const ed = monaco.editor.create(host, {
      model: null,
      theme: "forge-dark",
      fontFamily: FONT,
      fontSize: 13,
      lineHeight: 20,
      automaticLayout: true,
      minimap: { enabled: true, renderCharacters: false, maxColumn: 80 },
      scrollBeyondLastLine: false,
      smoothScrolling: true,
      padding: { top: 8 },
      renderWhitespace: "selection",
      bracketPairColorization: { enabled: true },
      guides: { bracketPairs: true, indentation: true },
      stickyScroll: { enabled: true },
      cursorBlinking: "smooth",
      cursorSmoothCaretAnimation: "on",
      fixedOverflowWidgets: true,
    });
    editorRef.current = ed;
    ed.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      const rel = activeRef.current;
      if (rel) void saveFile(rel).catch(() => undefined);
    });
    const subs = [
      ed.onDidChangeCursorSelection((e) => {
        const sel = e.selection;
        const model = ed.getModel();
        setCursor({
          line: sel.positionLineNumber,
          col: sel.positionColumn,
          selected: model && !sel.isEmpty() ? model.getValueLengthInRange(sel) : 0,
        });
      }),
      ed.onDidFocusEditorText(() => {
        markEditorFocused(pane.id);
        apply((d) => focusPane(d, pane.id));
      }),
    ];
    const unsubscribe = subscribeReveal(pane.id, () => {
      const rel = activeRef.current;
      const r = rel ? takeReveal(pane.id, rel) : null;
      if (r && ed.getModel()) applyReveal(ed, r);
    });
    const onWindowFocus = () => {
      if (activeRef.current) void checkExternalChange(wsRef.current, activeRef.current);
    };
    window.addEventListener("focus", onWindowFocus);
    return () => {
      window.removeEventListener("focus", onWindowFocus);
      unsubscribe();
      if (activeRef.current) viewStates.set(`${pane.id}:${activeRef.current}`, ed.saveViewState());
      subs.forEach((s) => s.dispose());
      ed.dispose();
      editorRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pane.id]);

  // Show the active file.
  useEffect(() => {
    const ed = editorRef.current;
    if (!ed) return;
    if (activeRef.current && activeRef.current !== state.active) {
      viewStates.set(`${pane.id}:${activeRef.current}`, ed.saveViewState());
    }
    activeRef.current = state.active;
    setFileError(null);
    if (!state.active) {
      ed.setModel(null);
      setMeta(null);
      return;
    }
    const rel = state.active;
    let cancelled = false;
    acquire(workspaceId, rel)
      .then((entry) => {
        if (cancelled) return;
        ed.setModel(entry.model);
        ed.updateOptions({ readOnly: !!entry.readOnly });
        const vs = viewStates.get(`${pane.id}:${rel}`);
        if (vs) ed.restoreViewState(vs);
        const r = takeReveal(pane.id, rel);
        if (r) applyReveal(ed, r);
        setMeta({
          language: languageName(entry.model.getLanguageId()),
          eol: entry.model.getEOL() === "\r\n" ? "CRLF" : "LF",
          readOnly: entry.readOnly,
        });
        void checkExternalChange(workspaceId, rel);
      })
      .catch((e) => {
        if (cancelled) return;
        ed.setModel(null);
        setMeta(null);
        const binary = e instanceof BinaryFileError;
        setFileError({ rel, binary, message: binary ? "Arquivos binários não podem ser editados." : errorMessage(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [workspaceId, state.active, pane.id, applyReveal]);

  useEffect(() => {
    if (focused && state.active) editorRef.current?.focus();
  }, [focused, state.active]);

  const closeTab = (rel: string, force = false) => {
    if (!force && dirty[fileKey(workspaceId, rel)]) {
      setConfirmClose(rel);
      return;
    }
    updateState((s) => {
      const idx = s.tabs.indexOf(rel);
      const tabs = s.tabs.filter((t) => t !== rel);
      const active = s.active === rel ? (tabs[Math.min(idx, tabs.length - 1)] ?? null) : s.active;
      return { tabs, active };
    });
    viewStates.delete(`${pane.id}:${rel}`);
    setTimeout(() => releaseUnreferenced(workspaceId, referencedFiles()), 0);
  };

  const activeKey = state.active ? fileKey(workspaceId, state.active) : null;
  const inConflict = !!(activeKey && conflicts[activeKey]);
  const showEditor = !!state.active && !fileError;

  return (
    <div className="flex h-full min-w-0 flex-col bg-bg-2">
      {/* Tabs */}
      {state.tabs.length > 0 && (
        <div className="flex h-8 shrink-0 items-stretch overflow-x-auto border-b border-border bg-bg" role="tablist" aria-label="Arquivos abertos">
          {state.tabs.map((rel) => {
            const key = fileKey(workspaceId, rel);
            const active = rel === state.active;
            const duplicate = state.tabs.filter((t) => basename(t) === basename(rel)).length > 1;
            return (
              <div
                key={rel}
                role="tab"
                aria-selected={active}
                title={rel}
                onClick={() => updateState((s) => ({ ...s, active: rel }))}
                onAuxClick={(e) => e.button === 1 && closeTab(rel)}
                className={cn(
                  "group relative flex max-w-[220px] shrink-0 cursor-default items-center gap-1.5 border-r border-border pr-1 pl-3 text-[12px]",
                  active ? "bg-bg-2 text-fg" : "text-muted hover:bg-surface hover:text-fg",
                )}
              >
                {active && <span className="absolute inset-x-0 top-0 h-px bg-accent" />}
                <span className="truncate">{basename(rel)}</span>
                {duplicate && <span className="truncate text-[10.5px] text-subtle">{dirname(rel)}</span>}
                {conflicts[key] && <TriangleAlert className="size-3 shrink-0 text-warning" />}
                <button
                  type="button"
                  aria-label={`Fechar ${basename(rel)}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(rel);
                  }}
                  className="relative flex size-5 shrink-0 items-center justify-center rounded hover:bg-elevated"
                >
                  {dirty[key] ? (
                    <>
                      <span className="size-2 rounded-full bg-fg/80 group-hover:hidden" />
                      <X className="hidden size-3 group-hover:block" />
                    </>
                  ) : (
                    <X className="size-3 opacity-0 group-hover:opacity-100" />
                  )}
                </button>
              </div>
            );
          })}
        </div>
      )}

      {/* Banners */}
      {showEditor && meta?.readOnly && (
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-border bg-warning/10 px-3 text-[11.5px] text-warning">
          <Lock className="size-3" /> {meta.readOnly}
        </div>
      )}
      {showEditor && inConflict && state.active && (
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-warning/10 px-3 text-[12px] text-warning">
          <TriangleAlert className="size-3.5" /> O arquivo foi alterado fora do FORGE e você tem alterações não salvas.
          <div className="ml-auto flex gap-1.5">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void reloadFromDisk(workspaceId, state.active!).catch((e) => toast.error("Falha ao recarregar", { description: errorMessage(e) }))}
            >
              Recarregar do disco
            </Button>
            <Button size="sm" variant="danger" onClick={() => void saveFile(state.active!, true).catch(() => undefined)}>
              Sobrescrever
            </Button>
          </div>
        </div>
      )}

      {/* Editor surface */}
      <div className="relative min-h-0 flex-1">
        <div ref={hostRef} className={cn("absolute inset-0", !showEditor && "invisible")} />
        {!state.active && (
          <EmptyState
            className="absolute inset-0"
            icon={<FileCode2 />}
            title="Nenhum arquivo aberto"
            description="Abra um arquivo pela árvore de Files, pela busca ou pelo Quick Open."
            action={
              <Button size="sm" onClick={() => openQuickOpen(true)}>
                Abrir arquivo <Kbd>{modKey}</Kbd>
                <Kbd>P</Kbd>
              </Button>
            }
          />
        )}
        {fileError && (
          <EmptyState className="absolute inset-0" icon={<FileWarning />} title={basename(fileError.rel)} description={fileError.message} />
        )}
      </div>

      {/* Status bar */}
      <div className="flex h-6 shrink-0 items-center gap-3 border-t border-border px-2.5 font-mono text-[10.5px] text-subtle">
        {state.active ? (
          <>
            <span className="min-w-0 truncate" title={state.active}>
              {state.active}
            </span>
            {activeKey && dirty[activeKey] && <span className="text-warning">● não salvo</span>}
            {meta && (
              <span className="ml-auto flex shrink-0 items-center gap-3">
                <span>
                  Ln {cursor.line}, Col {cursor.col}
                  {cursor.selected > 0 && ` (${cursor.selected} selecionados)`}
                </span>
                <span>UTF-8</span>
                <span>{meta.eol}</span>
                <span>{meta.language}</span>
              </span>
            )}
          </>
        ) : (
          <span>Editor</span>
        )}
      </div>

      <Dialog open={!!confirmClose} onOpenChange={(o) => !o && setConfirmClose(null)}>
        <DialogContent title="Salvar alterações?" description={confirmClose ?? undefined}>
          <div className="px-5 py-4 text-[13px] text-muted">
            O arquivo tem alterações não salvas.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmClose(null)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={async () => {
                  const rel = confirmClose!;
                  setConfirmClose(null);
                  closeTab(rel, true);
                  // Drop the unsaved buffer unless another pane still shows the file.
                  setTimeout(() => {
                    if (!referencedFiles().has(rel)) release(workspaceId, rel);
                  }, 0);
                }}
              >
                Descartar
              </Button>
              <Button
                variant="primary"
                onClick={async () => {
                  const rel = confirmClose!;
                  setConfirmClose(null);
                  try {
                    await saveFile(rel);
                    closeTab(rel, true);
                  } catch {
                    /* conflict/error already reported */
                  }
                }}
              >
                Salvar
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

