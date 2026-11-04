import type { MemoryInput, MemoryKind, MemoryScope } from "@/lib/ipc";

export const KINDS: MemoryKind[] = ["decision", "convention", "context", "history", "error", "preference", "doc"];

export const KIND_INFO: Record<MemoryKind, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "error" }> = {
  decision: { label: "Decisão", tone: "accent" },
  convention: { label: "Convenção", tone: "success" },
  context: { label: "Contexto", tone: "neutral" },
  history: { label: "Histórico", tone: "neutral" },
  error: { label: "Erro recorrente", tone: "error" },
  preference: { label: "Preferência", tone: "warning" },
  doc: { label: "Documentação", tone: "neutral" },
};

export const SCOPE_LABEL: Record<MemoryScope, string> = {
  workspace: "Workspace (só este projeto)",
  mission: "Missão",
  agent: "Agente",
  global: "Global (todos os projetos)",
};

/** Lowercased, deduplicated tags from a comma/space separated string. */
export function parseTags(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/[,\n]/)) {
    const t = raw.trim().replace(/^#+/, "").toLowerCase();
    if (t && !out.includes(t)) out.push(t);
  }
  return out;
}

/** Global items and agent items without a workspace reach every project. */
export function scopeNeedsCrossProjectConsent(v: Pick<MemoryInput, "scope" | "workspaceId">): boolean {
  return v.scope === "global" || (v.scope === "agent" && !v.workspaceId);
}
