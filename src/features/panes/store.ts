/**
 * Workspace sessions (tabs) and their pane layouts.
 *
 * Each workspace has 1..20 sessions; each session owns a `LayoutDoc`. Every
 * change is persisted to SQLite per session (debounced). Layout operations
 * themselves live in `layout.ts`.
 */
import { create } from "zustand";
import { toast } from "sonner";
import { api, errorMessage, type Preset } from "@/lib/ipc";
import { fileKey, useEditorStatus } from "@/features/editor/status";
import { closePane, defaultLayout, instantiate, panes, parseLayout, serializeLayout, toTemplate, type LayoutDoc } from "./layout";

export type SessionMeta = { id: string; name: string };

type LayoutState = {
  workspaceId: string | null;
  sessions: SessionMeta[];
  activeId: string | null;
  docs: Record<string, LayoutDoc>;
  /** Layout of the active session (derived, kept in sync). */
  doc: LayoutDoc | null;

  load: (workspaceId: string) => Promise<void>;
  apply: (fn: (doc: LayoutDoc) => LayoutDoc) => void;
  /** Applies `fn` to every session of the workspace (e.g. after a file rename). */
  applyAll: (fn: (doc: LayoutDoc) => LayoutDoc) => void;
  reset: () => void;
  applyTemplate: (template: LayoutDoc) => void;
  flush: () => Promise<void>;
  /** Closes a pane and releases its runtime resources (e.g. kills its terminal). */
  closePaneById: (paneId: string) => void;

  newSession: (template?: LayoutDoc, name?: string) => Promise<void>;
  duplicateSession: (id: string) => Promise<void>;
  renameSession: (id: string, name: string) => Promise<void>;
  closeSession: (id: string) => Promise<void>;
  switchSession: (id: string) => void;
  cycleSession: (delta: 1 | -1) => void;
  moveSession: (id: string, delta: 1 | -1) => Promise<void>;
};

const pending = new Map<string, string>();
let saveTimer: ReturnType<typeof setTimeout> | null = null;

async function persist() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = null;
  const jobs = [...pending.entries()];
  pending.clear();
  for (const [id, json] of jobs) {
    try {
      await api.sessionsSaveLayout(id, json);
    } catch (e) {
      toast.error("Falha ao salvar layout", { description: errorMessage(e) });
    }
  }
}

function schedule(sessionId: string, doc: LayoutDoc) {
  pending.set(sessionId, serializeLayout(doc));
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void persist(), 350);
}

// ── Terminal garbage collection ──────────────────────────────────────────
// Terminals live in the Rust core independently of the UI. Any terminal of the
// loaded workspace that no pane (in any session, incl. minimized) references
// anymore is killed after a grace period — this covers closing sessions,
// changing a pane's kind and applying layouts, while leaving time for "Undo".

/** Pane kinds backed by a PTY process. */
export const TERMINAL_KINDS = new Set<string>(["terminal", "claude", "codex", "gemini", "preview"]);
export const BROWSER_KINDS = new Set<string>(["browser", "preview"]);

const GC_DELAY_MS = 8000;
const GC_MIN_AGE_MS = 15000;
let gcTimer: ReturnType<typeof setTimeout> | null = null;

export function referencedTerminalIds(docs: Iterable<LayoutDoc>): Set<string> {
  const ids = new Set<string>();
  for (const doc of docs) {
    for (const p of [...panes(doc.root), ...doc.minimized.map((m) => m.pane)]) {
      if (TERMINAL_KINDS.has(p.kind) && typeof p.state?.terminalId === "string") ids.add(p.state.terminalId);
    }
  }
  return ids;
}

function scheduleTerminalGc() {
  if (gcTimer) clearTimeout(gcTimer);
  gcTimer = setTimeout(() => void runTerminalGc(), GC_DELAY_MS);
}

export function referencedBrowserTabIds(docs: Iterable<LayoutDoc>): Set<string> {
  const ids = new Set<string>();
  for (const doc of docs) {
    for (const p of [...panes(doc.root), ...doc.minimized.map((m) => m.pane)]) {
      if (BROWSER_KINDS.has(p.kind) && typeof p.state?.tabId === "string") ids.add(p.state.tabId);
    }
  }
  return ids;
}

async function runTerminalGc() {
  const { workspaceId, sessions, docs } = useLayout.getState();
  // Only collect once every session of the workspace is loaded.
  if (!workspaceId || sessions.length === 0 || sessions.some((s) => !docs[s.id])) return;
  const refs = referencedTerminalIds(Object.values(docs));
  try {
    const live = await api.terminalList(workspaceId);
    if (useLayout.getState().workspaceId !== workspaceId) return;
    for (const t of live) {
      if (!refs.has(t.id) && Date.now() - t.createdAt > GC_MIN_AGE_MS) await api.terminalKill(t.id);
    }
    const tabRefs = referencedBrowserTabIds(Object.values(docs));
    for (const t of await api.browserList(workspaceId)) {
      if (!tabRefs.has(t.id) && Date.now() - t.createdAt > GC_MIN_AGE_MS) await api.browserClose(t.id);
    }
  } catch {
    /* best effort */
  }
}

const fail = (title: string) => (e: unknown) => {
  toast.error(title, { description: errorMessage(e) });
};

function nextName(sessions: SessionMeta[]): string {
  const used = new Set(sessions.map((s) => s.name));
  for (let i = sessions.length + 1; ; i++) if (!used.has(`Sessão ${i}`)) return `Sessão ${i}`;
}

export const useLayout = create<LayoutState>((set, get) => {
  const setActive = (activeId: string | null, docs = get().docs) => set({ activeId, docs, doc: activeId ? (docs[activeId] ?? null) : null });

  return {
    workspaceId: null,
    sessions: [],
    activeId: null,
    docs: {},
    doc: null,

    load: async (workspaceId) => {
      await persist(); // flush the previous workspace first
      set({ workspaceId, sessions: [], activeId: null, docs: {}, doc: null });
      try {
        let state = await api.sessionsList(workspaceId);
        if (state.sessions.length === 0) {
          const s = await api.sessionsCreate(workspaceId, "Principal", serializeLayout(defaultLayout()));
          state = { sessions: [s], activeId: s.id };
        }
        if (get().workspaceId !== workspaceId) return; // switched meanwhile
        const docs: Record<string, LayoutDoc> = {};
        for (const s of state.sessions) {
          const parsed = parseLayout(s.layoutJson);
          docs[s.id] = parsed ?? defaultLayout();
          if (!parsed) schedule(s.id, docs[s.id]!); // repair invalid stored layout
        }
        set({ sessions: state.sessions.map(({ id, name }) => ({ id, name })) });
        setActive(state.activeId ?? state.sessions[0]!.id, docs);
        scheduleTerminalGc();
      } catch (e) {
        fail("Falha ao carregar sessões")(e);
      }
    },

    apply: (fn) => {
      const { doc, activeId, docs } = get();
      if (!doc || !activeId) return;
      const next = fn(doc);
      if (next === doc) return;
      set({ docs: { ...docs, [activeId]: next }, doc: next });
      schedule(activeId, next);
      scheduleTerminalGc();
    },

    applyAll: (fn) => {
      const { docs, activeId } = get();
      const next: Record<string, LayoutDoc> = {};
      let changed = false;
      for (const [id, doc] of Object.entries(docs)) {
        const d = fn(doc);
        next[id] = d;
        if (d !== doc) {
          changed = true;
          schedule(id, d);
        }
      }
      if (changed) set({ docs: next, doc: activeId ? (next[activeId] ?? null) : null });
    },

    closePaneById: (paneId) => {
      const doc = get().doc;
      if (!doc) return;
      const pane = [...panes(doc.root), ...doc.minimized.map((m) => m.pane)].find((p) => p.id === paneId);
      if (pane && TERMINAL_KINDS.has(pane.kind) && typeof pane.state?.terminalId === "string") {
        void api.terminalKill(pane.state.terminalId).catch(() => undefined);
      }
      if (pane && BROWSER_KINDS.has(pane.kind) && typeof pane.state?.tabId === "string") {
        void api.browserClose(pane.state.tabId).catch(() => undefined);
      }
      if (pane?.kind === "editor" && Array.isArray(pane.state?.tabs)) {
        const ws = get().workspaceId ?? "";
        const dirty = (pane.state.tabs as unknown[]).filter(
          (t): t is string => typeof t === "string" && !!useEditorStatus.getState().dirty[fileKey(ws, t)],
        );
        if (dirty.length) {
          toast.warning("Alterações não salvas foram mantidas", {
            description: `${dirty.join(", ")} — reabra o arquivo ou use “Salvar todos”.`,
          });
        }
      }
      get().apply((d) => closePane(d, paneId));
    },

    reset: () => get().apply(() => defaultLayout()),
    applyTemplate: (template) => get().apply(() => instantiate(template)),
    flush: persist,

    newSession: async (template, name) => {
      const { workspaceId, sessions, docs } = get();
      if (!workspaceId) return;
      const doc = template ? instantiate(template) : defaultLayout();
      try {
        const s = await api.sessionsCreate(workspaceId, name ?? nextName(sessions), serializeLayout(doc));
        if (get().workspaceId !== workspaceId) return;
        set({ sessions: [...get().sessions, { id: s.id, name: s.name }] });
        setActive(s.id, { ...docs, [s.id]: doc });
      } catch (e) {
        fail("Não foi possível criar a sessão")(e);
      }
    },

    duplicateSession: async (id) => {
      const { docs, sessions } = get();
      const src = docs[id];
      const meta = sessions.find((s) => s.id === id);
      if (!src || !meta) return;
      const { workspaceId } = get();
      if (!workspaceId) return;
      const doc = instantiate(src, true);
      try {
        const s = await api.sessionsCreate(workspaceId, `${meta.name} (cópia)`.slice(0, 60), serializeLayout(doc));
        set({ sessions: [...get().sessions, { id: s.id, name: s.name }] });
        setActive(s.id, { ...get().docs, [s.id]: doc });
      } catch (e) {
        fail("Não foi possível duplicar a sessão")(e);
      }
    },

    renameSession: async (id, name) => {
      try {
        const s = await api.sessionsRename(id, name);
        set({ sessions: get().sessions.map((x) => (x.id === id ? { ...x, name: s.name } : x)) });
      } catch (e) {
        fail("Não foi possível renomear")(e);
      }
    },

    closeSession: async (id) => {
      if (get().sessions.length <= 1) {
        toast.info("O workspace precisa de pelo menos uma sessão");
        return;
      }
      pending.delete(id);
      try {
        const state = await api.sessionsDelete(id);
        const docs = { ...get().docs };
        delete docs[id];
        set({ sessions: state.sessions.map(({ id, name }) => ({ id, name })) });
        setActive(state.activeId, docs);
        scheduleTerminalGc();
      } catch (e) {
        fail("Não foi possível fechar a sessão")(e);
      }
    },

    switchSession: (id) => {
      const { workspaceId, activeId, docs } = get();
      if (!workspaceId || id === activeId || !docs[id]) return;
      setActive(id);
      api.sessionsSetActive(workspaceId, id).catch(fail("Falha ao alternar sessão"));
    },

    cycleSession: (delta) => {
      const { sessions, activeId } = get();
      if (sessions.length < 2) return;
      const idx = sessions.findIndex((s) => s.id === activeId);
      get().switchSession(sessions[(idx + delta + sessions.length) % sessions.length]!.id);
    },

    moveSession: async (id, delta) => {
      const { workspaceId, sessions } = get();
      const idx = sessions.findIndex((s) => s.id === id);
      const to = idx + delta;
      if (!workspaceId || idx < 0 || to < 0 || to >= sessions.length) return;
      const reordered = [...sessions];
      const [moved] = reordered.splice(idx, 1);
      reordered.splice(to, 0, moved!);
      set({ sessions: reordered });
      try {
        await api.sessionsReorder(workspaceId, reordered.map((s) => s.id));
      } catch (e) {
        set({ sessions });
        fail("Não foi possível reordenar")(e);
      }
    },
  };
});

if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", () => void persist());
}

// ── Saved layouts (user presets) ──────────────────────────────────────────

type PresetsState = {
  items: Preset[];
  loaded: boolean;
  refresh: () => Promise<void>;
  saveCurrent: (name: string) => Promise<boolean>;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
};

export const usePresets = create<PresetsState>((set, get) => ({
  items: [],
  loaded: false,
  refresh: async () => set({ items: await api.presetsList(), loaded: true }),
  saveCurrent: async (name) => {
    const doc = useLayout.getState().doc;
    if (!doc) return false;
    try {
      await api.presetsCreate(name, serializeLayout(toTemplate(doc)));
      await get().refresh();
      return true;
    } catch (e) {
      fail("Não foi possível salvar o layout")(e);
      return false;
    }
  },
  rename: async (id, name) => {
    try {
      await api.presetsRename(id, name);
      await get().refresh();
    } catch (e) {
      fail("Não foi possível renomear o layout")(e);
    }
  },
  remove: async (id) => {
    try {
      await api.presetsDelete(id);
      await get().refresh();
    } catch (e) {
      fail("Não foi possível excluir o layout")(e);
    }
  },
}));
