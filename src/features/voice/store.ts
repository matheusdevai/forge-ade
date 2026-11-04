import { Channel } from "@tauri-apps/api/core";
import { create } from "zustand";
import { agentDraft } from "@/features/browser/input";
import { ensurePane } from "@/features/panes/ensure";
import { findPane, panes, setPaneState } from "@/features/panes/layout";
import { useLayout } from "@/features/panes/store";
import { openWorkspace } from "@/features/workspace/actions";
import { api, errorMessage, type PausePlan, type VoiceEvent } from "@/lib/ipc";
import { useApp } from "@/stores/app";
import { useWorkspaces } from "@/stores/data";
import { useSettings } from "@/stores/settings";
import { matchWorkspace, needsConfirmation, parseCommand, type Intent, type NavView, type VoicePane } from "./intents";

export type VoiceMode = "jarvis" | "clock";
export type Phase = "idle" | "starting" | "listening" | "thinking" | "result";

export type VoiceAction = { label: string; tone?: "primary" | "danger" | "ghost"; run: () => void | Promise<void> };
export type VoiceResult = {
  tone: "success" | "warning" | "error" | "neutral";
  title: string;
  detail?: string;
  actions?: VoiceAction[];
  /** Keep the HUD open until the user acts. */
  sticky?: boolean;
};

type State = {
  mode: VoiceMode | null;
  phase: Phase;
  partial: string;
  level: number;
  transcript: string;
  result: VoiceResult | null;
  target: { paneId: string; label: string } | null;
  start: (mode: VoiceMode) => Promise<void>;
  stop: () => void;
  cancel: () => void;
  dismiss: () => void;
};

const AGENT_KINDS = new Set(["agent", "claude", "codex", "gemini"]);
const PANE_LABEL: Record<string, string> = { agent: "Agente", claude: "Claude Code", codex: "Codex", gemini: "Gemini" };
const VIEW_LABEL: Record<NavView, string> = {
  dashboard: "Dashboard",
  missions: "Missões",
  agents: "Agentes",
  providers: "Providers",
  library: "Squads & Receitas",
  skills: "Skills",
  mcp: "MCP",
  memory: "Memória",
  settings: "Configurações",
};
const PANE_NAME: Record<VoicePane, string> = { terminal: "Terminal", browser: "Navegador", preview: "Prévia", git: "Git", search: "Busca", agent: "Agente" };

export const locale = () => useSettings.getState().values["voice.locale"] || "pt-BR";

function speak(text: string) {
  if (useSettings.getState().values["voice.speak"]) void api.voiceSpeak(text).catch(() => undefined);
}

function permissionHint(code: string): string | undefined {
  if (code === "speech-permission" || code === "mic-permission") return "Autorize em Configurações → Voz (o macOS mostra o pedido uma única vez).";
  if (code === "on-device") return "Desative “Somente no dispositivo” ou escolha um idioma com suporte local em Configurações → Voz.";
  return undefined;
}

let unlistenTimer: ReturnType<typeof setTimeout> | null = null;

export const useVoice = create<State>((set, get) => {
  const finish = (result: VoiceResult) => {
    set({ phase: "result", result });
    if (unlistenTimer) clearTimeout(unlistenTimer);
    if (!result.sticky && !result.actions?.length) unlistenTimer = setTimeout(() => get().phase === "result" && get().dismiss(), 4500);
  };

  const workspaceRoute = () => {
    const r = useApp.getState().route;
    return r.view === "workspace" ? r.id : null;
  };

  const sendToAgentPane = (text: string, send: boolean) => {
    const id = ensurePane("agent");
    if (!id) return;
    useLayout.getState().apply((d) => {
      const p = findPane(d, id);
      return p ? setPaneState(d, id, { ...(p.state ?? {}), draft: text, draftSend: send }) : d;
    });
  };

  const runIntent = async (intent: Intent) => {
    const ws = workspaceRoute();
    switch (intent.kind) {
      case "navigate": {
        const view = intent.view;
        useApp.getState().navigate(view === "settings" ? { view: "settings", section: "general" } : { view });
        speak(`Abrindo ${VIEW_LABEL[view]}`);
        return finish({ tone: "success", title: `Abrindo ${VIEW_LABEL[view]}` });
      }
      case "openWorkspace": {
        const items = useWorkspaces.getState().items;
        const choose = (title: string): VoiceResult => ({
          tone: "neutral",
          title,
          sticky: true,
          actions: items.slice(0, 6).map((w) => ({ label: w.name, run: () => void openWorkspace(w.id).then(() => get().dismiss()) })),
        });
        if (items.length === 0) return finish({ tone: "warning", title: "Nenhum workspace cadastrado", detail: "Abra uma pasta com ⌘O." });
        if (!intent.query) return finish(choose("Qual projeto?"));
        const w = matchWorkspace(items, intent.query);
        if (!w) return finish(choose(`Não encontrei “${intent.query}”. Qual projeto?`));
        await openWorkspace(w.id);
        speak(`Abrindo ${w.name}`);
        return finish({ tone: "success", title: `Abrindo ${w.name}` });
      }
      case "openPane": {
        if (!ws) return finish({ tone: "warning", title: "Abra um workspace primeiro", detail: `O painel ${PANE_NAME[intent.pane]} vive dentro de um workspace.` });
        ensurePane(intent.pane);
        return finish({ tone: "success", title: `${PANE_NAME[intent.pane]} aberto` });
      }
      case "createMission": {
        useApp.getState().openNewMission(ws, intent.briefing);
        speak("Revise a missão e confirme");
        return finish({ tone: "success", title: "Nova missão preenchida", detail: intent.briefing ? `“${intent.briefing}” — revise e confirme no diálogo.` : "Descreva o objetivo no diálogo." });
      }
      case "askAgent": {
        if (!ws) return finish({ tone: "warning", title: "Abra um workspace primeiro", detail: "O agente trabalha dentro de um workspace." });
        sendToAgentPane(intent.prompt, useSettings.getState().values["voice.autoSend"]);
        return finish({ tone: "success", title: "Pedido enviado ao painel do agente", detail: intent.prompt });
      }
      case "verifyFrontend": {
        const doc = useLayout.getState().doc;
        const tab = doc
          ? [...panes(doc.root), ...doc.minimized.map((m) => m.pane)].find((p) => (p.kind === "preview" || p.kind === "browser") && typeof p.state?.tabId === "string")
          : undefined;
        if (!ws || !tab) {
          return finish({
            tone: "warning",
            title: "Nenhuma página aberta para verificar",
            detail: "Abra a Prévia (rodar o servidor) ou o Navegador com a página do frontend.",
            actions: ws ? [{ label: "Abrir Prévia", tone: "primary", run: () => void (ensurePane("preview"), get().dismiss()) }] : undefined,
          });
        }
        set({ phase: "thinking" });
        try {
          const r = await api.browserVerify(tab.state!.tabId as string);
          const msg = r.ok ? "Nenhum erro encontrado" : `${r.errors.length} erro(s) e ${r.warnings.length} aviso(s)`;
          speak(msg);
          return finish({
            tone: r.ok ? "success" : "error",
            title: msg,
            detail: r.url,
            sticky: !r.ok,
            actions: r.ok ? undefined : [{ label: "Enviar ao agente para corrigir", tone: "primary", run: () => void (sendToAgentPane(agentDraft(r.summary), false), get().dismiss()) }],
          });
        } catch (e) {
          return finish({ tone: "error", title: "Falha ao verificar a página", detail: errorMessage(e) });
        }
      }
      case "pauseAgents": {
        let plan: PausePlan;
        try {
          plan = await api.agentsPauseAll(true);
        } catch (e) {
          return finish({ tone: "error", title: "Falha ao listar agentes", detail: errorMessage(e) });
        }
        if (plan.missions.length === 0 && plan.runs.length === 0) return finish({ tone: "neutral", title: "Nenhum agente em execução" });
        const parts = [
          plan.missions.length ? `${plan.missions.length} missão(ões) serão pausadas (tarefas em andamento terminam): ${plan.missions.map((m) => m.label).join(", ")}` : "",
          plan.runs.length ? `${plan.runs.length} execução(ões) avulsa(s) serão interrompidas.` : "",
        ].filter(Boolean);
        speak("Confirma pausar os agentes?");
        return finish({
          tone: "warning",
          title: "Pausar todos os agentes?",
          detail: parts.join(" "),
          sticky: true,
          actions: [
            {
              label: "Confirmar",
              tone: "danger",
              run: async () => {
                try {
                  const done = await api.agentsPauseAll(false);
                  speak("Agentes pausados");
                  finish({ tone: "success", title: "Agentes pausados", detail: `${done.missions.length} missão(ões) pausada(s), ${done.runs.length} execução(ões) interrompida(s).` });
                } catch (e) {
                  finish({ tone: "error", title: "Falha ao pausar", detail: errorMessage(e) });
                }
              },
            },
            { label: "Cancelar", tone: "ghost", run: () => get().dismiss() },
          ],
        });
      }
      case "unknown": {
        return finish({
          tone: "neutral",
          title: "Não entendi o comando",
          detail: `“${intent.text}”. Exemplos: “Forge, abra o projeto X”, “crie uma missão para…”, “verifique os erros do frontend”, “pause todos os agentes”.`,
          sticky: true,
          actions: ws ? [{ label: "Enviar ao agente", run: () => void (sendToAgentPane(intent.text, false), get().dismiss()) }] : undefined,
        });
      }
    }
  };

  const handleFinal = async (text: string) => {
    const t = text.trim();
    set({ transcript: t, partial: t, phase: "thinking" });
    if (!t) return finish({ tone: "neutral", title: "Não ouvi nada", detail: "Fale após o sinal; a escuta para sozinha depois de uma pausa." });
    const { mode, target } = get();
    if (mode === "clock" && target) {
      const doc = useLayout.getState().doc;
      const pane = doc ? findPane(doc, target.paneId) : null;
      const autoSend = useSettings.getState().values["voice.autoSend"];
      if (!pane) return finish({ tone: "warning", title: "O painel de destino foi fechado" });
      if (pane.kind === "agent") {
        useLayout.getState().apply((d) => {
          const p = findPane(d, pane.id);
          return p ? setPaneState(d, pane.id, { ...(p.state ?? {}), draft: t, draftSend: autoSend }) : d;
        });
      } else if (typeof pane.state?.terminalId === "string") {
        await api.terminalWrite(pane.state.terminalId, t + (autoSend ? "\r" : "")).catch(() => undefined);
      } else {
        return finish({ tone: "warning", title: "O agente desse painel não está em execução" });
      }
      return finish({ tone: "success", title: autoSend ? `Enviado para ${target.label}` : `Ditado em ${target.label}`, detail: t });
    }
    // Push-to-talk already addresses Jarvis, so the wake word is optional.
    // Sensitive intents (needsConfirmation) ask before running, inside runIntent.
    const { intent } = parseCommand(t);
    if (needsConfirmation(intent) && intent.kind !== "pauseAgents") return finish({ tone: "warning", title: "Confirmação necessária" });
    return runIntent(intent);
  };

  return {
    mode: null,
    phase: "idle",
    partial: "",
    level: 0,
    transcript: "",
    result: null,
    target: null,

    start: async (mode) => {
      const cur = get();
      if (cur.phase === "listening" || cur.phase === "starting") {
        get().stop();
        return;
      }
      if (unlistenTimer) clearTimeout(unlistenTimer);
      let target: State["target"] = null;
      if (mode === "clock") {
        const doc = useLayout.getState().doc;
        const focused = doc?.focusedId ? findPane(doc, doc.focusedId) : null;
        const pane = focused && AGENT_KINDS.has(focused.kind) ? focused : doc ? panes(doc.root).find((p) => AGENT_KINDS.has(p.kind)) : null;
        if (useApp.getState().route.view !== "workspace" || !pane) {
          set({ mode, phase: "result", result: { tone: "warning", title: "Nenhum agente para ditar", detail: "Abra um painel Agente, Claude Code, Codex ou Gemini no workspace (⌘I) e tente de novo." } });
          return;
        }
        target = { paneId: pane.id, label: PANE_LABEL[pane.kind] ?? "agente" };
      }
      set({ mode, phase: "starting", partial: "", transcript: "", level: 0, result: null, target });
      void api.voiceStopSpeaking().catch(() => undefined);
      const ch = new Channel<VoiceEvent>();
      let gotFinal = false;
      ch.onmessage = (ev) => {
        switch (ev.type) {
          case "listening":
            set({ phase: "listening" });
            break;
          case "partial":
            set({ partial: ev.text });
            break;
          case "level":
            set({ level: ev.rms });
            break;
          case "final":
            if (!gotFinal) {
              gotFinal = true;
              void handleFinal(ev.text);
            }
            break;
          case "error":
            gotFinal = true;
            finish({ tone: "error", title: ev.message, detail: permissionHint(ev.code), sticky: true });
            break;
          case "exit":
            if (!gotFinal && (get().phase === "listening" || get().phase === "starting")) set({ phase: "idle", mode: null });
            break;
        }
      };
      try {
        await api.voiceListen(locale(), useSettings.getState().values["voice.onDevice"], ch);
      } catch (e) {
        finish({ tone: "error", title: "Não foi possível ouvir", detail: errorMessage(e), sticky: true });
      }
    },
    stop: () => void api.voiceStop().catch(() => undefined),
    cancel: () => {
      void api.voiceCancel().catch(() => undefined);
      set({ phase: "idle", mode: null, partial: "", result: null });
    },
    dismiss: () => set({ phase: "idle", mode: null, result: null, partial: "", transcript: "" }),
  };
});
