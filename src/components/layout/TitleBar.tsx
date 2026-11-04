import { Mic, Search } from "lucide-react";
import { Kbd, Tooltip } from "@/components/ui";
import { useVoice } from "@/features/voice/store";
import { isMac, modKey } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useWorkspaces } from "@/stores/data";

function useTitle() {
  const route = useApp((s) => s.route);
  const ws = useWorkspaces((s) => (route.view === "workspace" ? s.items.find((w) => w.id === route.id) : undefined));
  switch (route.view) {
    case "dashboard":
      return "Dashboard";
    case "missions":
    case "mission":
      return "Missões";
    case "providers":
      return "Providers";
    case "agents":
      return "Agentes";
    case "library":
      return "Squads & Receitas";
    case "skills":
      return "Skills";
    case "mcp":
      return "MCP";
    case "memory":
      return "Memória";
    case "media":
      return "Mídia";
    case "autopilot":
      return "Autopilot";
    case "observability":
      return "Observabilidade";
    case "settings":
      return "Configurações";
    case "workspace":
      return ws?.name ?? "Workspace";
  }
}

/** Custom title bar — the whole strip is a window drag region (macOS overlay style). */
export function TitleBar() {
  const title = useTitle();
  const setPaletteOpen = useApp((s) => s.setPaletteOpen);
  return (
    <div className="relative flex h-10 shrink-0 items-center border-b border-border bg-bg px-3" data-tauri-drag-region>
      <span className="pointer-events-none text-xs text-subtle" style={{ paddingLeft: isMac ? 0 : 4 }}>
        {title}
      </span>
      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        className="absolute left-1/2 flex h-7 w-[min(420px,40vw)] -translate-x-1/2 items-center gap-2 rounded-md border border-border bg-bg-2 px-2.5 text-xs text-subtle transition-colors hover:border-border-strong hover:text-muted"
      >
        <Search className="size-3.5" />
        Buscar ou executar comando…
        <span className="ml-auto flex gap-0.5">
          <Kbd>{modKey}</Kbd>
          <Kbd>K</Kbd>
        </span>
      </button>
      <VoiceButton />
    </div>
  );
}

function VoiceButton() {
  const phase = useVoice((s) => s.phase);
  const mode = useVoice((s) => s.mode);
  const active = mode === "jarvis" && (phase === "listening" || phase === "starting");
  return (
    <div className="absolute right-3">
      <Tooltip content={`Jarvis — comando de voz (${modKey}⇧J)`}>
        <button
          type="button"
          onClick={() => void useVoice.getState().start("jarvis")}
          aria-label="Falar com o Jarvis"
          className={`grid size-7 place-items-center rounded-md border transition-colors ${active ? "border-accent/60 bg-accent/15 text-accent" : "border-border text-subtle hover:text-fg"}`}
        >
          <Mic className="size-3.5" />
        </button>
      </Tooltip>
    </div>
  );
}
