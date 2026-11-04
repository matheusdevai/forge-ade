import { describe, expect, it } from "vitest";
import { titleFromBriefing } from "@/features/missions/NewMissionDialog";
import { fold, matchWorkspace, needsConfirmation, parseCommand } from "./intents";

const intent = (s: string) => parseCommand(s).intent;

describe("Jarvis intents", () => {
  it("handles the spec examples", () => {
    expect(intent("Forge, abra o projeto.")).toEqual({ kind: "openWorkspace", query: null });
    expect(intent("Forge, crie uma missão para implementar pagamentos.")).toEqual({ kind: "createMission", briefing: "implementar pagamentos" });
    expect(intent("Forge, verifique os erros do frontend.")).toEqual({ kind: "verifyFrontend" });
    expect(intent("Forge, pause todos os agentes.")).toEqual({ kind: "pauseAgents" });
  });

  it("detects the wake word and tolerates transcription variants", () => {
    expect(parseCommand("Forge, abra as missões").wake).toBe(true);
    expect(parseCommand("abra as missões").wake).toBe(false);
    expect(intent("Ok Forge abra a memória")).toEqual({ kind: "navigate", view: "memory" });
    expect(intent("forj, abra o terminal")).toEqual({ kind: "openPane", pane: "terminal" });
  });

  it("opens workspaces by name, views and panes", () => {
    expect(intent("Forge, abra o projeto Ofertabot")).toEqual({ kind: "openWorkspace", query: "ofertabot" });
    expect(intent("abrir workspace chamado legal hub")).toEqual({ kind: "openWorkspace", query: "legal hub" });
    expect(intent("vá para as configurações")).toEqual({ kind: "navigate", view: "settings" });
    expect(intent("mostre os agentes")).toEqual({ kind: "navigate", view: "agents" });
    expect(intent("abra o agente")).toEqual({ kind: "openPane", pane: "agent" });
    expect(intent("abra o navegador")).toEqual({ kind: "openPane", pane: "browser" });
    expect(intent("open the preview")).toEqual({ kind: "openPane", pane: "preview" });
    expect(intent("Forge, open missions")).toEqual({ kind: "navigate", view: "missions" });
  });

  it("asks the agent with the original wording", () => {
    expect(intent("Forge, pergunte ao agente quais testes estão falhando?")).toEqual({ kind: "askAgent", prompt: "quais testes estão falhando?" });
    expect(intent("ask the agent to explain the auth flow")).toEqual({ kind: "askAgent", prompt: "explain the auth flow" });
  });

  it("creates missions in English and keeps accents in the briefing", () => {
    expect(intent("Forge, create a mission to add dark mode")).toEqual({ kind: "createMission", briefing: "add dark mode" });
    expect(intent("nova missão: migração do banco de dados")).toEqual({ kind: "createMission", briefing: "migração do banco de dados" });
  });

  it("never guesses", () => {
    expect(intent("Forge, delete tudo")).toEqual({ kind: "unknown", text: "Forge, delete tudo" });
    expect(intent("bom dia")).toMatchObject({ kind: "unknown" });
    expect(intent("")).toMatchObject({ kind: "unknown" });
  });

  it("requires confirmation only for sensitive actions", () => {
    expect(needsConfirmation({ kind: "pauseAgents" })).toBe(true);
    expect(needsConfirmation({ kind: "navigate", view: "missions" })).toBe(false);
  });

  it("matches spoken workspace names", () => {
    const ws = [{ name: "ofertabot-ai" }, { name: "forge" }, { name: "LegalHub" }, { name: "jurisassist" }];
    expect(matchWorkspace(ws, "ofertabot")?.name).toBe("ofertabot-ai");
    expect(matchWorkspace(ws, "Forge")?.name).toBe("forge");
    expect(matchWorkspace(ws, "legal hub")?.name).toBe("LegalHub");
    expect(matchWorkspace(ws, "nada")).toBeNull();
    expect(fold("Missão: Ação!")).toBe("missao acao");
  });

  it("derives a mission title from a briefing", () => {
    expect(titleFromBriefing("implementar pagamentos")).toBe("Implementar pagamentos");
    expect(titleFromBriefing("a".repeat(80)).length).toBeLessThanOrEqual(58);
  });
});
