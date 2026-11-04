import { useVoice } from "@/features/voice/store";
import { useEffect } from "react";
import { addPane, cycleFocus, splitPane, toggleMaximize } from "@/features/panes/layout";
import { isMac } from "@/lib/utils";
import { useLayout } from "@/features/panes/store";
import { pickAndOpenWorkspace } from "@/features/workspace/actions";
import { ensurePane } from "@/features/panes/ensure";
import { saveAllWithFeedback } from "@/features/editor/saveAll";
import { useApp } from "@/stores/app";

/**
 * Global keyboard shortcuts (Cmd on macOS, Ctrl elsewhere).
 *   K palette · N new mission · O open workspace · , settings
 *   1 dashboard · 2 missions · ] / [ next/prev pane · Shift+Enter maximize pane
 *   I agent pane · T new session · Shift+] / Shift+[ next/prev session · J new terminal below
 *   P quick open · Shift+F search · Shift+G git · Alt+S save all (S alone is handled by the editor)
 *
 * On Windows/Linux the modifier is Ctrl, which shells also use (Ctrl+K, Ctrl+N,
 * Ctrl+[ …). While a terminal has focus there, only Ctrl+Shift+K (palette) is
 * handled so the shell keeps its keys. On macOS, Cmd never reaches the shell.
 */
export function useGlobalHotkeys() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && useVoice.getState().phase !== "idle") {
        const v = useVoice.getState();
        if (v.phase === "listening" || v.phase === "starting") v.cancel();
        else v.dismiss();
        return;
      }
      const mod = isMac ? e.metaKey : e.ctrlKey;
      if (!mod) return;
      if (e.altKey) {
        if (e.code === "KeyS") {
          e.preventDefault();
          e.stopPropagation();
          void saveAllWithFeedback();
        }
        return;
      }
      const inMonaco = e.target instanceof Element && !!e.target.closest(".monaco-editor");
      const inTerminal = e.target instanceof Element && !!e.target.closest(".xterm");
      if (!isMac && inTerminal) {
        if (e.shiftKey && e.key.toLowerCase() === "k") {
          e.preventDefault();
          useApp.getState().setPaletteOpen(true);
        }
        return;
      }
      const app = useApp.getState();
      const layout = useLayout.getState();
      const inWs = app.route.view === "workspace" && layout.doc;
      const key = e.key.toLowerCase();

      const handled = (() => {
        if (key === "k" && !e.shiftKey) return app.setPaletteOpen(!app.paletteOpen), true;
        if (key === "p" && !e.shiftKey && app.route.view === "workspace") return app.setQuickOpen(!app.quickOpen), true;
        if (app.paletteOpen || app.newMission.open || app.quickOpen) return false;
        if (inWs && e.shiftKey && key === "f") return void ensurePane("search"), true;
        if (inWs && e.shiftKey && key === "g") return void ensurePane("git"), true;
        if (inWs && e.shiftKey && key === "b") return void ensurePane("browser"), true;
        if (e.shiftKey && key === "j") return void useVoice.getState().start("jarvis"), true;
        if (e.shiftKey && key === "d") return void useVoice.getState().start("clock"), true;
        // Brackets indent/outdent inside the editor.
        if (inMonaco && (e.code === "BracketLeft" || e.code === "BracketRight")) return false;
        if (key === "n" && !e.shiftKey) return app.openNewMission(app.route.view === "workspace" ? app.route.id : null), true;
        if (key === "o" && !e.shiftKey) return void pickAndOpenWorkspace(), true;
        if (key === ",") return app.navigate({ view: "settings", section: "general" }), true;
        if (key === "1") return app.navigate({ view: "dashboard" }), true;
        if (key === "2") return app.navigate({ view: "missions" }), true;
        if (key === "3") return app.navigate({ view: "agents" }), true;
        if (key === "4") return app.navigate({ view: "providers" }), true;
        if (key === "5") return app.navigate({ view: "library" }), true;
        if (key === "6") return app.navigate({ view: "skills" }), true;
        if (key === "7") return app.navigate({ view: "mcp" }), true;
        if (key === "8") return app.navigate({ view: "memory" }), true;
        if (key === "9") return app.navigate({ view: "media" }), true;
        if (key === "0") return app.navigate({ view: "autopilot" }), true;
        if (inWs && key === "t" && !e.shiftKey) return void layout.newSession(), true;
        if (inWs && key === "i" && !e.shiftKey) return void ensurePane("agent"), true;
        if (inWs && key === "j" && !e.shiftKey) {
          const id = layout.doc?.focusedId;
          layout.apply((d) => (id ? splitPane(d, id, "vertical", "terminal") : addPane(d, "terminal")));
          return true;
        }
        if (inWs && e.shiftKey && e.code === "BracketRight") return layout.cycleSession(1), true;
        if (inWs && e.shiftKey && e.code === "BracketLeft") return layout.cycleSession(-1), true;
        if (inWs && !e.shiftKey && e.code === "BracketRight") return layout.apply((d) => cycleFocus(d, 1)), true;
        if (inWs && !e.shiftKey && e.code === "BracketLeft") return layout.apply((d) => cycleFocus(d, -1)), true;
        if (inWs && e.shiftKey && key === "enter") {
          const id = layout.doc?.focusedId;
          if (id) layout.apply((d) => toggleMaximize(d, id));
          return true;
        }
        return false;
      })();
      if (handled) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    // Capture phase: app shortcuts win over widgets (Monaco binds Cmd+K chords).
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}
