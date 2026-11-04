/**
 * Lightweight editor state that does NOT import Monaco: dirty/conflict flags
 * per file, used by tabs, the close guard and file operations.
 */
import { create } from "zustand";

export const fileKey = (workspaceId: string, relPath: string) => `${workspaceId}\u0000${relPath}`;
export const splitKey = (key: string) => {
  const [workspaceId = "", relPath = ""] = key.split("\u0000");
  return { workspaceId, relPath };
};

type StatusState = {
  dirty: Record<string, true>;
  conflicts: Record<string, true>;
  setDirty: (key: string, dirty: boolean) => void;
  setConflict: (key: string, conflict: boolean) => void;
  forget: (key: string) => void;
};

function toggle(map: Record<string, true>, key: string, on: boolean): Record<string, true> {
  if (on === !!map[key]) return map;
  const next = { ...map };
  if (on) next[key] = true;
  else delete next[key];
  return next;
}

export const useEditorStatus = create<StatusState>((set) => ({
  dirty: {},
  conflicts: {},
  setDirty: (key, dirty) => set((s) => ({ dirty: toggle(s.dirty, key, dirty) })),
  setConflict: (key, conflict) => set((s) => ({ conflicts: toggle(s.conflicts, key, conflict) })),
  forget: (key) => set((s) => ({ dirty: toggle(s.dirty, key, false), conflicts: toggle(s.conflicts, key, false) })),
}));

export function dirtyFiles(): { workspaceId: string; relPath: string }[] {
  return Object.keys(useEditorStatus.getState().dirty).map(splitKey);
}
