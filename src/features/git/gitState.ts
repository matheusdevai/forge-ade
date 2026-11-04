/**
 * Git status per workspace + a runner for git operations that reports errors,
 * refreshes status and tells the rest of the app the repo/worktree changed.
 */
import { create } from "zustand";
import { toast } from "sonner";
import { api, errorMessage, type GitStatus } from "@/lib/ipc";

export const GIT_CHANGED = "forge:git-changed";
export const FILES_CHANGED = "forge:files-changed";

type Entry = { status: GitStatus | null; error: string | null; loading: boolean };
type GitState = {
  byWs: Record<string, Entry>;
  busy: Record<string, string | null>;
  refresh: (ws: string) => Promise<void>;
};

const inflight = new Map<string, Promise<void>>();

export const useGit = create<GitState>((set, get) => ({
  byWs: {},
  busy: {},
  refresh: (ws) => {
    const running = inflight.get(ws);
    if (running) return running;
    const p = (async () => {
      const prev = get().byWs[ws];
      set({ byWs: { ...get().byWs, [ws]: { status: prev?.status ?? null, error: null, loading: true } } });
      try {
        const status = await api.gitStatus(ws);
        set({ byWs: { ...get().byWs, [ws]: { status, error: null, loading: false } } });
      } catch (e) {
        set({ byWs: { ...get().byWs, [ws]: { status: prev?.status ?? null, error: errorMessage(e), loading: false } } });
      } finally {
        inflight.delete(ws);
      }
    })();
    inflight.set(ws, p);
    return p;
  },
}));

/** Repo path → workspace-relative path (null when outside the workspace). */
export function toWorkspacePath(status: GitStatus | null | undefined, repoPath: string): string | null {
  const prefix = status?.prefix ?? "";
  if (!prefix) return repoPath;
  return repoPath.startsWith(prefix) ? repoPath.slice(prefix.length) : null;
}

/**
 * Runs a git operation with busy state and feedback. `worktreeChanged` makes
 * open editor buffers re-check the disk (branch switch, pull, stash, discard).
 */
export async function gitRun<T>(
  ws: string,
  label: string,
  fn: () => Promise<T>,
  opts: { success?: string | ((r: T) => string); worktreeChanged?: boolean } = {},
): Promise<T | undefined> {
  if (useGit.getState().busy[ws]) {
    toast.info("Aguarde a operação Git em andamento");
    return undefined;
  }
  useGit.setState((s) => ({ busy: { ...s.busy, [ws]: label } }));
  try {
    const r = await fn();
    if (opts.success) toast.success(typeof opts.success === "function" ? opts.success(r) : opts.success);
    return r;
  } catch (e) {
    const msg = errorMessage(e);
    toast.error(label + " falhou", { description: msg.length > 600 ? msg.slice(0, 600) + "…" : msg, duration: 8000 });
    return undefined;
  } finally {
    useGit.setState((s) => ({ busy: { ...s.busy, [ws]: null } }));
    await useGit.getState().refresh(ws);
    window.dispatchEvent(new CustomEvent(GIT_CHANGED, { detail: { workspaceId: ws } }));
    if (opts.worktreeChanged) {
      const models = await import("@/features/editor/models").catch(() => null);
      await models?.checkAllOpen(ws);
    }
  }
}
