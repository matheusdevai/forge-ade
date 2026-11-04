import { create } from "zustand";

export type SettingsSection = "general" | "appearance" | "voice" | "layouts" | "processes" | "environment" | "brand" | "data" | "about";

export type Route =
  | { view: "dashboard" }
  | { view: "missions" }
  | { view: "mission"; id: string }
  | { view: "providers" }
  | { view: "agents" }
  | { view: "library"; tab?: "squads" | "recipes" }
  | { view: "skills"; id?: string }
  | { view: "mcp"; id?: string }
  | { view: "memory"; workspaceId?: string }
  | { view: "media" }
  | { view: "autopilot" }
  | { view: "observability"; tab?: "overview" | "processes" | "timeline" | "audit" }
  | { view: "workspace"; id: string }
  | { view: "settings"; section: SettingsSection };

type AppState = {
  route: Route;
  paletteOpen: boolean;
  quickOpen: boolean;
  newMission: { open: boolean; workspaceId: string | null; briefing?: string };
  navigate: (route: Route) => void;
  setPaletteOpen: (open: boolean) => void;
  setQuickOpen: (open: boolean) => void;
  openNewMission: (workspaceId?: string | null, briefing?: string) => void;
  closeNewMission: () => void;
};

export const useApp = create<AppState>((set) => ({
  route: { view: "dashboard" },
  paletteOpen: false,
  quickOpen: false,
  newMission: { open: false, workspaceId: null },
  navigate: (route) => set({ route, paletteOpen: false }),
  setPaletteOpen: (paletteOpen) => set({ paletteOpen, ...(paletteOpen ? { quickOpen: false } : {}) }),
  setQuickOpen: (quickOpen) => set({ quickOpen, ...(quickOpen ? { paletteOpen: false } : {}) }),
  openNewMission: (workspaceId = null, briefing) => set({ newMission: { open: true, workspaceId, briefing }, paletteOpen: false }),
  closeNewMission: () => set((s) => ({ newMission: { ...s.newMission, open: false } })),
}));
