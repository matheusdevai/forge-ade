/**
 * Editor actions usable from anywhere (explorer, search, quick open, palette)
 * without loading Monaco.
 */
import { toast } from "sonner";
import { addPane, panes, restorePane, setPaneState, splitPane, type LayoutDoc, type PaneNode } from "@/features/panes/layout";
import { useLayout } from "@/features/panes/store";

export type EditorState = { tabs: string[]; active: string | null };
export type Reveal = { line: number; column: number; length?: number };

export function readEditorState(raw: Record<string, unknown> | undefined): EditorState {
  const tabs = Array.isArray(raw?.tabs) ? [...new Set((raw.tabs as unknown[]).filter((t): t is string => typeof t === "string"))] : [];
  const active = typeof raw?.active === "string" && tabs.includes(raw.active) ? raw.active : (tabs[tabs.length - 1] ?? null);
  return { tabs, active };
}

/** Pending cursor reveals, consumed by the editor pane once the file is shown. */
const pendingReveals = new Map<string, { relPath: string; reveal: Reveal }>();
export function takeReveal(paneId: string, relPath: string): Reveal | null {
  const r = pendingReveals.get(paneId);
  if (!r || r.relPath !== relPath) return null;
  pendingReveals.delete(paneId);
  return r.reveal;
}

/** Lets a mounted editor pane react to a reveal for the file it already shows. */
const revealSubscribers = new Map<string, () => void>();
export function subscribeReveal(paneId: string, cb: () => void): () => void {
  revealSubscribers.set(paneId, cb);
  return () => {
    if (revealSubscribers.get(paneId) === cb) revealSubscribers.delete(paneId);
  };
}

let lastEditorPaneId: string | null = null;
export function markEditorFocused(paneId: string) {
  lastEditorPaneId = paneId;
}

function pickEditorPane(doc: LayoutDoc): PaneNode | null {
  const list = panes(doc.root);
  const editors = list.filter((p) => p.kind === "editor");
  return (
    editors.find((p) => p.id === doc.focusedId) ??
    editors.find((p) => p.id === lastEditorPaneId) ??
    editors[0] ??
    null
  );
}

/** Opens `relPath` in an editor pane of the active session, creating one if needed. */
export function openFileInEditor(relPath: string, reveal?: Reveal) {
  const { doc, apply } = useLayout.getState();
  if (!doc) return;
  let next = doc;
  let target = pickEditorPane(doc);
  if (!target) {
    const minimized = doc.minimized.find((m) => m.pane.kind === "editor");
    if (minimized) next = restorePane(doc, minimized.pane.id);
    else if (!doc.root) next = addPane(doc, "editor");
    else next = splitPane(doc, doc.focusedId ?? panes(doc.root)[0]!.id, "horizontal", "editor");
    target = pickEditorPane(next);
    if (!target) {
      toast.error("Limite de panes atingido", { description: "Feche um pane para abrir o editor." });
      return;
    }
  }
  const pane = target;
  const state = readEditorState(pane.state);
  const tabs = state.tabs.includes(relPath) ? state.tabs : [...state.tabs, relPath];
  if (reveal) pendingReveals.set(pane.id, { relPath, reveal });
  lastEditorPaneId = pane.id;
  const withTabs = setPaneState(next, pane.id, { ...(pane.state ?? {}), tabs, active: relPath });
  apply(() => ({
    ...withTabs,
    focusedId: pane.id,
    maximizedId: withTabs.maximizedId && withTabs.maximizedId !== pane.id ? null : withTabs.maximizedId,
  }));
  if (reveal) queueMicrotask(() => revealSubscribers.get(pane.id)?.());
}

function mapEditorTabs(doc: LayoutDoc, fn: (state: EditorState) => EditorState | null): LayoutDoc {
  let out = doc;
  for (const p of panes(doc.root)) {
    if (p.kind !== "editor") continue;
    const after = fn(readEditorState(p.state));
    if (after) out = setPaneState(out, p.id, { ...(p.state ?? {}), ...after });
  }
  const minimized = doc.minimized.map((m) => {
    if (m.pane.kind !== "editor") return m;
    const after = fn(readEditorState(m.pane.state));
    return after ? { ...m, pane: { ...m.pane, state: { ...(m.pane.state ?? {}), ...after } } } : m;
  });
  if (minimized.some((m, i) => m !== doc.minimized[i])) out = { ...out, minimized };
  return out;
}

const under = (path: string, prefix: string) => path === prefix || path.startsWith(prefix + "/");

/** Rewrites tab paths in every session after a file/folder rename. */
export function renameTabs(from: string, to: string) {
  useLayout.getState().applyAll((doc) =>
    mapEditorTabs(doc, (s) => {
      if (!s.tabs.some((t) => under(t, from))) return null;
      const re = (t: string) => (under(t, from) ? to + t.slice(from.length) : t);
      return { tabs: s.tabs.map(re), active: s.active ? re(s.active) : null };
    }),
  );
}

/** Closes tabs of a deleted file/folder in every session. */
export function closeTabsUnder(path: string) {
  useLayout.getState().applyAll((doc) =>
    mapEditorTabs(doc, (s) => {
      if (!s.tabs.some((t) => under(t, path))) return null;
      const tabs = s.tabs.filter((t) => !under(t, path));
      return { tabs, active: s.active && !under(s.active, path) ? s.active : (tabs[tabs.length - 1] ?? null) };
    }),
  );
}

/** Every file open in an editor tab of the loaded workspace (any session). */
export function referencedFiles(): Set<string> {
  const out = new Set<string>();
  for (const doc of Object.values(useLayout.getState().docs)) {
    for (const p of [...panes(doc.root), ...doc.minimized.map((m) => m.pane)]) {
      if (p.kind === "editor") readEditorState(p.state).tabs.forEach((t) => out.add(t));
    }
  }
  return out;
}
