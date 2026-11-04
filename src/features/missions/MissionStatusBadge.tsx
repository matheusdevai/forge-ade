import { Badge } from "@/components/ui";
import type { MissionMode, MissionStatus } from "@/lib/ipc";

const STATUS: Record<MissionStatus, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "error" }> = {
  draft: { label: "Rascunho", tone: "neutral" },
  planned: { label: "Planejada", tone: "accent" },
  running: { label: "Em execução", tone: "accent" },
  paused: { label: "Pausada", tone: "warning" },
  completed: { label: "Concluída", tone: "success" },
  failed: { label: "Falhou", tone: "error" },
  cancelled: { label: "Cancelada", tone: "neutral" },
};

export function MissionStatusBadge({ status }: { status: MissionStatus }) {
  const s = STATUS[status];
  return <Badge tone={s.tone}>{s.label}</Badge>;
}

export const MODE_INFO: Record<MissionMode, { label: string; description: string }> = {
  free: { label: "Free", description: "Você conduz: revisa e decide cada tarefa." },
  squad: { label: "Squad", description: "O Maestro coordena e um agente revisor avalia cada entrega; você decide." },
  agentic: { label: "Agentic", description: "O Maestro decide: aprova, refaz e reatribui sozinho, dentro dos limites. Plano e integração continuam com você." },
};
