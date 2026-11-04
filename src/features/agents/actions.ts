import { findPane, setPaneState } from "@/features/panes/layout";
import { ensurePane } from "@/features/panes/ensure";
import { useLayout } from "@/features/panes/store";
import type { ProviderId } from "@/lib/ipc";

/** Opens (or focuses) the provider's pane in `mode`, restarting it if it was running in another mode. */
export function launchAgentPane(providerId: ProviderId, mode: "agent" | "login") {
  const id = ensurePane(providerId);
  if (!id) return;
  useLayout.getState().apply((d) => {
    const p = findPane(d, id);
    if (!p) return d;
    const state = p.state ?? {};
    if ((state.launchMode ?? "agent") === mode) return d;
    const running = typeof state.terminalId === "string";
    const n = typeof state.launchNonce === "number" ? state.launchNonce : 0;
    return setPaneState(d, id, { ...state, launchMode: mode, launchNonce: running ? n + 1 : n });
  });
}
