import type { Routine, RoutineAction } from "@/lib/ipc";

export const DAYS = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];

/** Human summary of when a routine runs. */
export function triggerSummary(r: Pick<Routine, "trigger" | "intervalMin" | "dailyTime" | "daysMask">): string {
  switch (r.trigger) {
    case "manual":
      return "Só manualmente";
    case "commit":
      return "A cada novo commit";
    case "interval": {
      const m = r.intervalMin ?? 0;
      if (m % 1440 === 0) return `A cada ${m / 1440} dia(s)`;
      if (m % 60 === 0) return `A cada ${m / 60} h`;
      return `A cada ${m} min`;
    }
    case "daily": {
      const mask = r.daysMask;
      const days = mask === 127 ? "todo dia" : mask === 0b0011111 ? "dias úteis" : DAYS.filter((_, i) => mask & (1 << i)).join(", ");
      return `${days} às ${r.dailyTime ?? "—"}`;
    }
  }
}

const ACTION_LABEL: Record<RoutineAction["type"], string> = {
  agent: "Agente",
  script: "Script",
  command: "Comando",
  mission: "Missão de receita",
  verify: "Verificar página",
};

export function actionSummary(a: RoutineAction): string {
  switch (a.type) {
    case "agent":
      return `${ACTION_LABEL.agent} (${a.permission === "edit" ? "pode editar" : "leitura"}): ${a.prompt}`;
    case "script":
      return `${ACTION_LABEL.script}: ${a.script}`;
    case "command":
      return `${ACTION_LABEL.command}: ${a.command}`;
    case "mission":
      return `${ACTION_LABEL.mission}: ${a.title}`;
    case "verify":
      return `${ACTION_LABEL.verify}: ${a.url}`;
  }
}

export const actionLabel = (t: RoutineAction["type"]) => ACTION_LABEL[t];

export function duration(start: number, end: number | null): string {
  if (!end) return "—";
  const s = Math.round((end - start) / 1000);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}min ${s % 60}s`;
}
