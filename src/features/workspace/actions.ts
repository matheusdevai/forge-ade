import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/ipc";
import { useApp } from "@/stores/app";
import { useActivity, useWorkspaces } from "@/stores/data";
import { useSettings } from "@/stores/settings";

/** Opens a registered workspace: validates the directory, records the session and navigates. */
export async function openWorkspace(id: string) {
  try {
    const ws = await api.workspacesOpen(id);
    useWorkspaces.getState().markOpened(ws);
    useSettings.setState((s) => ({ values: { ...s.values, "session.lastWorkspaceId": ws.id } }));
    useApp.getState().navigate({ view: "workspace", id: ws.id });
    void useActivity.getState().refresh();
  } catch (e) {
    toast.error("Não foi possível abrir o workspace", { description: errorMessage(e) });
  }
}

/** Shows the native directory picker, registers the folder and opens it. */
export async function pickAndOpenWorkspace() {
  try {
    const defaultPath = useSettings.getState().values["workspace.defaultDir"] ?? undefined;
    const selected = await open({ directory: true, multiple: false, defaultPath, title: "Abrir workspace" });
    if (typeof selected !== "string") return;
    const ws = await useWorkspaces.getState().add(selected);
    await openWorkspace(ws.id);
  } catch (e) {
    toast.error("Não foi possível adicionar o workspace", { description: errorMessage(e) });
  }
}

export async function pickDirectory(title: string): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title });
  return typeof selected === "string" ? selected : null;
}
