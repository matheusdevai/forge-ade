/**
 * Text models shared by every editor pane (two panes on the same file edit the
 * same buffer, like in VS Code). Tracks the saved version, the on-disk mtime
 * for conflict detection, and read-only reasons.
 */
import { toast } from "sonner";
import { api, IpcError } from "@/lib/ipc";
import { monaco } from "./monaco";
import { fileKey, useEditorStatus } from "./status";

export const MAX_EDITOR_BYTES = 5 * 1024 * 1024;

export type ModelEntry = {
  workspaceId: string;
  relPath: string;
  model: monaco.editor.ITextModel;
  savedVersion: number;
  mtimeMs: number;
  readOnly: string | null;
};

export class BinaryFileError extends Error {}

const entries = new Map<string, ModelEntry>();
const loading = new Map<string, Promise<ModelEntry>>();

function uriFor(workspaceId: string, relPath: string) {
  return monaco.Uri.from({ scheme: "forge", authority: workspaceId, path: "/" + relPath });
}

function track(entry: ModelEntry) {
  const key = fileKey(entry.workspaceId, entry.relPath);
  entry.model.onDidChangeContent(() => {
    useEditorStatus.getState().setDirty(key, entry.model.getAlternativeVersionId() !== entry.savedVersion);
  });
}

export function getEntry(workspaceId: string, relPath: string): ModelEntry | undefined {
  return entries.get(fileKey(workspaceId, relPath));
}

export async function acquire(workspaceId: string, relPath: string): Promise<ModelEntry> {
  const key = fileKey(workspaceId, relPath);
  const existing = entries.get(key);
  if (existing && !existing.model.isDisposed()) return existing;
  const pending = loading.get(key);
  if (pending) return pending;
  const p = (async () => {
    const file = await api.fsReadText(workspaceId, relPath, MAX_EDITOR_BYTES);
    if (file.binary || file.content === null) throw new BinaryFileError("Arquivo binário");
    const uri = uriFor(workspaceId, relPath);
    monaco.editor.getModel(uri)?.dispose();
    const model = monaco.editor.createModel(file.content, undefined, uri);
    const entry: ModelEntry = {
      workspaceId,
      relPath,
      model,
      savedVersion: model.getAlternativeVersionId(),
      mtimeMs: file.mtimeMs,
      readOnly: file.truncated ? `Arquivo maior que ${MAX_EDITOR_BYTES / 1024 / 1024} MB — aberto somente leitura (parcial)` : null,
    };
    track(entry);
    entries.set(key, entry);
    return entry;
  })();
  loading.set(key, p);
  try {
    return await p;
  } finally {
    loading.delete(key);
  }
}

/** Saves the buffer. Throws IpcError(kind "conflict") if the file changed on disk, unless `force`. */
export async function save(workspaceId: string, relPath: string, force = false): Promise<void> {
  const entry = getEntry(workspaceId, relPath);
  if (!entry || entry.readOnly) return;
  const version = entry.model.getAlternativeVersionId();
  const meta = await api.fsWriteText(workspaceId, relPath, entry.model.getValue(), force ? null : entry.mtimeMs);
  entry.mtimeMs = meta.mtimeMs;
  entry.savedVersion = version;
  const key = fileKey(workspaceId, relPath);
  useEditorStatus.getState().setDirty(key, entry.model.getAlternativeVersionId() !== version);
  useEditorStatus.getState().setConflict(key, false);
  window.dispatchEvent(new CustomEvent("forge:files-changed", { detail: { workspaceId } }));
}

/** Replaces the buffer with the file on disk (undoable via Ctrl/Cmd+Z). */
export async function reloadFromDisk(workspaceId: string, relPath: string): Promise<void> {
  const entry = getEntry(workspaceId, relPath);
  if (!entry) return;
  const file = await api.fsReadText(workspaceId, relPath, MAX_EDITOR_BYTES);
  if (file.content === null) return;
  entry.model.pushEditOperations([], [{ range: entry.model.getFullModelRange(), text: file.content }], () => null);
  entry.mtimeMs = file.mtimeMs;
  entry.savedVersion = entry.model.getAlternativeVersionId();
  const key = fileKey(workspaceId, relPath);
  useEditorStatus.getState().setDirty(key, false);
  useEditorStatus.getState().setConflict(key, false);
}

/**
 * Checks whether the file changed on disk. Clean buffers are reloaded
 * silently; dirty buffers are flagged as conflicting.
 */
export async function checkExternalChange(workspaceId: string, relPath: string): Promise<void> {
  const entry = getEntry(workspaceId, relPath);
  if (!entry) return;
  try {
    const meta = await api.fsStat(workspaceId, relPath);
    if (meta.mtimeMs === entry.mtimeMs) return;
    const key = fileKey(workspaceId, relPath);
    if (useEditorStatus.getState().dirty[key]) useEditorStatus.getState().setConflict(key, true);
    else await reloadFromDisk(workspaceId, relPath);
  } catch (e) {
    if (e instanceof IpcError && e.kind === "not_found") {
      toast.warning("Arquivo removido do disco", { description: relPath });
    }
  }
}

/** Re-checks every open buffer of a workspace against the disk (after git operations). */
export async function checkAllOpen(workspaceId: string): Promise<void> {
  const list = [...entries.values()].filter((e) => e.workspaceId === workspaceId);
  await Promise.all(list.map((e) => checkExternalChange(workspaceId, e.relPath)));
}

export function release(workspaceId: string, relPath: string) {
  const key = fileKey(workspaceId, relPath);
  const entry = entries.get(key);
  if (!entry) return;
  entry.model.dispose();
  entries.delete(key);
  useEditorStatus.getState().forget(key);
}

/** Moves buffers after a rename (file or folder prefix), keeping unsaved edits. */
export function renamePaths(workspaceId: string, from: string, to: string) {
  for (const [key, entry] of [...entries]) {
    if (entry.workspaceId !== workspaceId) continue;
    const matches = entry.relPath === from || entry.relPath.startsWith(from + "/");
    if (!matches) continue;
    const newRel = to + entry.relPath.slice(from.length);
    const wasDirty = !!useEditorStatus.getState().dirty[key];
    const value = entry.model.getValue();
    const model = monaco.editor.createModel(value, undefined, uriFor(workspaceId, newRel));
    const moved: ModelEntry = { ...entry, relPath: newRel, model, savedVersion: wasDirty ? -1 : model.getAlternativeVersionId() };
    entry.model.dispose();
    entries.delete(key);
    useEditorStatus.getState().forget(key);
    track(moved);
    entries.set(fileKey(workspaceId, newRel), moved);
    if (wasDirty) useEditorStatus.getState().setDirty(fileKey(workspaceId, newRel), true);
    void api.fsStat(workspaceId, newRel).then((m) => (moved.mtimeMs = m.mtimeMs)).catch(() => undefined);
  }
}

/** Disposes clean buffers no editor tab references anymore. */
export function releaseUnreferenced(workspaceId: string, referenced: Set<string>) {
  for (const entry of [...entries.values()]) {
    if (entry.workspaceId !== workspaceId || referenced.has(entry.relPath)) continue;
    if (useEditorStatus.getState().dirty[fileKey(workspaceId, entry.relPath)]) continue;
    release(workspaceId, entry.relPath);
  }
}

export async function saveAll(): Promise<{ failed: string[] }> {
  const failed: string[] = [];
  for (const key of Object.keys(useEditorStatus.getState().dirty)) {
    const entry = entries.get(key);
    if (!entry) continue;
    try {
      await save(entry.workspaceId, entry.relPath);
    } catch {
      failed.push(entry.relPath);
    }
  }
  return { failed };
}
