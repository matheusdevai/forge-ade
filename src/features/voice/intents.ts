/**
 * Jarvis command interpreter: turns a transcript (pt-BR or en) into an
 * intent. Deterministic rules — no LLM call, no guessing: anything not
 * understood is returned as `unknown` with the original text.
 */

export type NavView = "dashboard" | "missions" | "agents" | "providers" | "library" | "skills" | "mcp" | "memory" | "settings";
export type VoicePane = "terminal" | "browser" | "preview" | "git" | "search" | "agent";

export type Intent =
  | { kind: "navigate"; view: NavView }
  | { kind: "openWorkspace"; query: string | null }
  | { kind: "openPane"; pane: VoicePane }
  | { kind: "createMission"; briefing: string }
  | { kind: "verifyFrontend" }
  | { kind: "pauseAgents" }
  | { kind: "askAgent"; prompt: string }
  | { kind: "unknown"; text: string };

/** Lowercase, no accents, punctuation → spaces, single spaces. */
export function fold(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

const WAKE = /^(?:(?:ei|ok|hey|oi|ola)\s+)?(?:forge|forj|forje|forg|forgi|forte|fortes|forja)\b\s*/;

export function stripWake(folded: string): { text: string; wake: boolean } {
  const m = folded.match(WAKE);
  return m ? { text: folded.slice(m[0].length), wake: true } : { text: folded, wake: false };
}

/** Removes the wake word and leading punctuation from the original text. */
function originalAfterWake(original: string): string {
  return original.replace(/^\s*(?:(?:ei|ok|hey|oi|olá|ola)[\s,]+)?(?:forge|forj|forje|forg|forgi|forja)[\s,:.!-]*/i, "").trim();
}

const VIEWS: [RegExp, NavView][] = [
  [/\b(missoes|missions)\b/, "missions"],
  [/\b(agentes|agents)\b/, "agents"],
  [/\b(providers?|provedores)\b/, "providers"],
  [/\b(biblioteca|library|squads|receitas|recipes)\b/, "library"],
  [/\b(skills|habilidades)\b/, "skills"],
  [/\b(mcp|servidores mcp)\b/, "mcp"],
  [/\b(memoria|memory|memorias)\b/, "memory"],
  [/\b(configuracoes|configuracao|ajustes|settings|preferencias)\b/, "settings"],
  [/\b(dashboard|inicio|painel inicial|home)\b/, "dashboard"],
];

const PANES: [RegExp, VoicePane][] = [
  [/\bterminal\b/, "terminal"],
  [/\b(navegador|browser)\b/, "browser"],
  [/\b(preview|previa|pre visualizacao)\b/, "preview"],
  [/\bgit\b/, "git"],
  [/\b(busca|pesquisa|search)\b/, "search"],
  [/\b(agente|agent)\b/, "agent"],
];

const OPEN = /^(?:abra|abrir|abre|abri|open|mostre|mostrar|mostra|show|ir para|va para|vai para|go to|navegue para|entre em|entrar em)\b\s*/;

export function parseCommand(transcript: string): { intent: Intent; wake: boolean } {
  const { text, wake } = stripWake(fold(transcript));
  const original = originalAfterWake(transcript);
  const intent = ((): Intent => {
    if (!text) return { kind: "unknown", text: transcript.trim() };

    if (/\b(pause|pausar|pausa|pare|parar|para|interrompa|interromper|stop|halt)\b.*\b(agentes|agents|tudo|everything|all agents)\b/.test(text)) {
      return { kind: "pauseAgents" };
    }

    if (/\b(verifique|verificar|verifica|cheque|checar|checa|confira|conferir|check|verify)\b.*\b(erros?|errors?|frontend|front end|pagina|page|site|console)\b/.test(text)) {
      return { kind: "verifyFrontend" };
    }

    if (/\b(crie|criar|cria|crio|nova|novo|create|start|new|abra|abrir)\b.*\b(missao|mission)\b/.test(text)) {
      const m = original.match(/\b(?:miss[aã]o|mission)\b[\s,:]*(?:(?:para|pra|de|sobre|to|for|about|that)\b\s*)?(.*)$/i);
      const briefing = (m?.[1] ?? "").replace(/[.!?\s]+$/, "").trim();
      return { kind: "createMission", briefing };
    }

    const ask = original.match(/^(?:pergunte|pergunta|perguntar|diga|fale|peça|peca|pede|ask|tell)\s+(?:ao|para o|pro|pra o|o|the)?\s*(?:agente|agent)\b[\s,:]*(?:(?:que|to|that)\b\s*)?(.+)$/i);
    if (ask?.[1]) return { kind: "askAgent", prompt: ask[1].trim() };

    const open = text.match(OPEN);
    if (open) {
      const rest = text.slice(open[0].length).replace(/^(o|a|os|as|the|meu|minha|my)\s+/, "");
      const ws = rest.match(/^(?:projeto|workspace|project|pasta)\b\s*(.*)$/);
      if (ws) {
        const q = ws[1]!.replace(/^(do|da|de|chamado|called|named)\s+/, "").trim();
        return { kind: "openWorkspace", query: q || null };
      }
      for (const [re, view] of VIEWS) if (re.test(rest)) return { kind: "navigate", view };
      for (const [re, pane] of PANES) if (re.test(rest)) return { kind: "openPane", pane };
    }
    return { kind: "unknown", text: transcript.trim() };
  })();
  return { intent, wake };
}

/** Sensitive intents need an explicit confirmation before running. */
export const needsConfirmation = (i: Intent) => i.kind === "pauseAgents";

/** Best workspace match for a spoken name (null when ambiguous/none). */
export function matchWorkspace<T extends { name: string }>(items: T[], query: string): T | null {
  const q = fold(query);
  if (!q) return null;
  const exact = items.filter((w) => fold(w.name) === q);
  if (exact.length === 1) return exact[0]!;
  const partial = items.filter((w) => fold(w.name).includes(q) || q.includes(fold(w.name)));
  if (partial.length === 1) return partial[0]!;
  const words = q.split(" ").filter((w) => w.length > 2);
  const scored = items
    .map((w) => ({ w, n: words.filter((x) => fold(w.name).includes(x)).length }))
    .filter((x) => x.n > 0)
    .sort((a, b) => b.n - a.n);
  if (scored.length && (scored.length === 1 || scored[0]!.n > scored[1]!.n)) return scored[0]!.w;
  return null;
}
