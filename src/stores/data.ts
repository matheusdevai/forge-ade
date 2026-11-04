/**
 * Domain stores backed by the Rust core. Components read from these; every
 * mutation goes through the API and then refreshes the affected slices.
 */
import { create } from "zustand";
import { api, type ActivityEvent, type Mission, type NewMission, type ToolStatus, type Workspace } from "@/lib/ipc";

// ── Workspaces ────────────────────────────────────────────────────────────
type WorkspacesState = {
  items: Workspace[];
  loaded: boolean;
  refresh: () => Promise<void>;
  add: (path: string) => Promise<Workspace>;
  markOpened: (ws: Workspace) => void;
  setPinned: (id: string, pinned: boolean) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
};

export const useWorkspaces = create<WorkspacesState>((set, get) => ({
  items: [],
  loaded: false,
  refresh: async () => set({ items: await api.workspacesList(), loaded: true }),
  add: async (path) => {
    const ws = await api.workspacesAdd(path);
    await get().refresh();
    void useActivity.getState().refresh();
    return ws;
  },
  markOpened: (ws) => set({ items: [ws, ...get().items.filter((w) => w.id !== ws.id)] }),
  setPinned: async (id, pinned) => {
    await api.workspacesSetPinned(id, pinned);
    await get().refresh();
  },
  rename: async (id, name) => {
    await api.workspacesRename(id, name);
    await get().refresh();
  },
  remove: async (id) => {
    await api.workspacesRemove(id);
    await get().refresh();
    void useActivity.getState().refresh();
  },
}));

// ── Missions ──────────────────────────────────────────────────────────────
type MissionsState = {
  items: Mission[];
  loaded: boolean;
  refresh: () => Promise<void>;
  create: (input: NewMission) => Promise<Mission>;
  cancel: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
};

export const useMissions = create<MissionsState>((set, get) => ({
  items: [],
  loaded: false,
  refresh: async () => set({ items: await api.missionsList(null, 200), loaded: true }),
  create: async (input) => {
    const m = await api.missionsCreate(input);
    await get().refresh();
    void useActivity.getState().refresh();
    return m;
  },
  cancel: async (id) => {
    await api.missionsCancel(id);
    await get().refresh();
    void useActivity.getState().refresh();
  },
  remove: async (id) => {
    await api.missionsDelete(id);
    await get().refresh();
    void useActivity.getState().refresh();
  },
}));

// ── Activity ──────────────────────────────────────────────────────────────
type ActivityState = { items: ActivityEvent[]; refresh: () => Promise<void> };

export const useActivity = create<ActivityState>((set) => ({
  items: [],
  refresh: async () => set({ items: await api.activityRecent(30) }),
}));

// ── Environment ───────────────────────────────────────────────────────────
type EnvironmentState = {
  tools: ToolStatus[];
  scanning: boolean;
  scannedAt: number | null;
  scan: () => Promise<void>;
};

export const useEnvironment = create<EnvironmentState>((set, get) => ({
  tools: [],
  scanning: false,
  scannedAt: null,
  scan: async () => {
    if (get().scanning) return;
    set({ scanning: true });
    try {
      set({ tools: await api.detectEnvironment(), scannedAt: Date.now() });
    } finally {
      set({ scanning: false });
    }
  },
}));
