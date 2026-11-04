import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import type { AutopilotEvent } from "@/lib/ipc";
import { isDesktop } from "@/lib/ipc";
import { useApp } from "@/stores/app";

/** In-app alert for failed routines (the OS notification is sent by the core). */
export function useAutopilotAlerts() {
  useEffect(() => {
    if (!isDesktop()) return;
    const un = listen<AutopilotEvent>("forge://autopilot-changed", ({ payload: e }) => {
      if (e.status !== "failed") return;
      toast.error(e.autoDisabled ? `Rotina “${e.routineName}” desativada após falhas` : `Rotina “${e.routineName}” falhou`, {
        description: e.message ?? undefined,
        action: { label: "Ver", onClick: () => useApp.getState().navigate({ view: "autopilot" }) },
      });
    });
    return () => void un.then((f) => f());
  }, []);
}
