import { toast } from "sonner";
import { addPane, panes, restorePane, splitPane, type Orientation, type PaneKind } from "./layout";
import { useLayout } from "./store";

/**
 * Focuses an existing pane of `kind` in the active session (restoring it if
 * minimized) or creates one next to the focused pane. Returns its id.
 */
export function ensurePane(kind: PaneKind, orientation: Orientation = "horizontal"): string | null {
  const { doc, apply } = useLayout.getState();
  if (!doc) return null;
  const existing = panes(doc.root).find((p) => p.kind === kind);
  if (existing) {
    apply((d) => ({ ...d, focusedId: existing.id, maximizedId: d.maximizedId === existing.id ? d.maximizedId : null }));
    return existing.id;
  }
  const minimized = doc.minimized.find((m) => m.pane.kind === kind);
  let next = doc;
  if (minimized) next = restorePane(doc, minimized.pane.id);
  else if (!doc.root) next = addPane(doc, kind);
  else next = splitPane(doc, doc.focusedId ?? panes(doc.root)[0]!.id, orientation, kind);
  if (next === doc) {
    toast.error("Limite de panes atingido");
    return null;
  }
  apply(() => next);
  return next.focusedId;
}
