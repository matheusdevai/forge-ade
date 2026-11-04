/**
 * Pointer-based pane drag and drop.
 *
 * We deliberately avoid HTML5 drag events: inside Tauri (notably WebView2 on
 * Windows) native file-drop handling intercepts them. Pointer events work the
 * same on every platform and give full control over the drop preview.
 */
import { create } from "zustand";
import { dropZoneFor, movePane, type DropZone, type PaneKind } from "./layout";
import { useLayout } from "./store";

type DragState = {
  source: { paneId: string; kind: PaneKind } | null;
  target: { paneId: string; zone: DropZone } | null;
  x: number;
  y: number;
};

export const usePaneDrag = create<DragState>(() => ({ source: null, target: null, x: 0, y: 0 }));

const THRESHOLD = 4;

function hitTest(x: number, y: number, sourceId: string): DragState["target"] {
  const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-pane-id]");
  const paneId = el?.dataset.paneId;
  if (!el || !paneId || paneId === sourceId) return null;
  const r = el.getBoundingClientRect();
  return { paneId, zone: dropZoneFor((x - r.left) / r.width, (y - r.top) / r.height) };
}

/** Call from a pointerdown on a pane's drag handle. */
export function beginPaneDrag(e: React.PointerEvent, paneId: string, kind: PaneKind) {
  if (e.button !== 0 || useLayout.getState().doc?.maximizedId) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let active = false;

  const move = (ev: PointerEvent) => {
    if (!active) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < THRESHOLD) return;
      active = true;
      document.body.style.cursor = "grabbing";
      document.documentElement.dataset.paneDragging = "true";
      window.getSelection()?.removeAllRanges();
    }
    usePaneDrag.setState({ source: { paneId, kind }, x: ev.clientX, y: ev.clientY, target: hitTest(ev.clientX, ev.clientY, paneId) });
  };

  const end = (commit: boolean) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", cancel);
    document.body.style.cursor = "";
    delete document.documentElement.dataset.paneDragging;
    const { target } = usePaneDrag.getState();
    usePaneDrag.setState({ source: null, target: null });
    if (commit && active && target) {
      useLayout.getState().apply((d) => movePane(d, paneId, target.paneId, target.zone));
    }
  };
  const up = () => end(true);
  const cancel = () => end(false);
  const key = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") {
      ev.preventDefault();
      ev.stopPropagation();
      cancel();
    }
  };

  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("keydown", key, true);
  window.addEventListener("blur", cancel);
}
