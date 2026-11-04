import { useEffect, useState } from "react";
import { Check, Loader2, TriangleAlert, Wand2, X } from "lucide-react";
import { Badge, Button } from "@/components/ui";
import { api, errorMessage, type HarnessPreference, type Recommendation } from "@/lib/ipc";
import { cn } from "@/lib/utils";

export const PREFERENCES: { id: HarnessPreference; label: string }[] = [
  { id: "economy", label: "Economia" },
  { id: "balanced", label: "Equilíbrio" },
  { id: "quality", label: "Qualidade" },
];

const ACTIVITY: Record<string, string> = {
  question: "Pergunta",
  review: "Revisão",
  plan: "Planejamento",
  docs: "Documentação",
  tests: "Testes",
  bugfix: "Correção",
  refactor: "Refatoração",
  feature: "Implementação",
  design: "Design",
};
const SIZE: Record<string, string> = { small: "pequena", medium: "média", large: "grande" };

/** Recommendation card: what the Harness suggests for `text`, with reasons. */
export function HarnessPanel({
  text,
  preference,
  onPreference,
  onApply,
  onClose,
}: {
  text: string;
  preference: HarnessPreference;
  onPreference: (p: HarnessPreference) => void;
  onApply: (r: Recommendation) => void;
  onClose: () => void;
}) {
  const [rec, setRec] = useState<Recommendation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    api
      .harnessRecommend(text, preference)
      .then((r) => alive && setRec(r))
      .catch((e) => alive && setError(errorMessage(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [text, preference]);

  const c = rec?.classification;
  return (
    <div className="mb-2 rounded-lg border border-accent/30 bg-surface">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Wand2 className="size-3.5 text-accent" />
        <span className="text-[12.5px] font-medium">Harness</span>
        {c && (
          <span className="text-[11px] text-muted">
            {ACTIVITY[c.activity] ?? c.activity}
            {c.domain ? ` · ${c.domain}` : ""} · porte {SIZE[c.complexity]}
          </span>
        )}
        <div className="ml-auto flex rounded-md border border-border bg-bg p-0.5">
          {PREFERENCES.map((p) => (
            <button
              key={p.id}
              onClick={() => onPreference(p.id)}
              className={cn("rounded px-1.5 py-0.5 text-[10.5px]", preference === p.id ? "bg-elevated text-fg" : "text-muted hover:text-fg")}
            >
              {p.label}
            </button>
          ))}
        </div>
        <Button size="icon" variant="ghost" className="size-6" onClick={onClose} aria-label="Fechar">
          <X />
        </Button>
      </div>
      <div className="max-h-64 overflow-auto px-3 py-2">
        {loading && !rec ? (
          <p className="flex items-center gap-2 text-[12px] text-muted">
            <Loader2 className="size-3.5 animate-spin" /> Analisando a tarefa e os providers…
          </p>
        ) : error ? (
          <p className="text-[12px] text-error">{error}</p>
        ) : rec ? (
          <>
            <div className="flex flex-wrap gap-1">
              <Badge tone="accent">{rec.profile?.name ?? "sem perfil"}</Badge>
              <Badge>{rec.providerId ?? "sem provider"}</Badge>
              <Badge>{rec.model ?? "modelo padrão do CLI"}</Badge>
              <Badge tone={rec.permission === "edit" ? "warning" : "success"}>{rec.permission === "edit" ? "pode editar" : "somente leitura"}</Badge>
              <Badge>até {rec.maxTurns} turnos</Badge>
              {rec.useWorktree && <Badge tone="accent">worktree</Badge>}
              {rec.skills.map((s) => (
                <Badge key={s.id}>skill: {s.name}</Badge>
              ))}
              {rec.mcp.map((s) => (
                <Badge key={s.id}>mcp: {s.name}</Badge>
              ))}
            </div>
            <ul className="mt-2 space-y-0.5 text-[11.5px] leading-relaxed text-muted">
              {rec.reasons.map((r, i) => (
                <li key={i}>· {r}</li>
              ))}
            </ul>
            {rec.warnings.length > 0 && (
              <ul className="mt-1.5 space-y-0.5 text-[11.5px] leading-relaxed text-warning">
                {rec.warnings.map((w, i) => (
                  <li key={i} className="flex gap-1">
                    <TriangleAlert className="mt-0.5 size-3 shrink-0" /> {w}
                  </li>
                ))}
              </ul>
            )}
          </>
        ) : null}
      </div>
      {rec && (
        <div className="flex justify-end gap-2 border-t border-border px-3 py-1.5">
          <Button size="sm" variant="ghost" onClick={onClose}>
            Ignorar
          </Button>
          <Button size="sm" variant="primary" disabled={!rec.profile || !rec.providerId} onClick={() => onApply(rec)}>
            <Check /> Aplicar nesta conversa
          </Button>
        </div>
      )}
    </div>
  );
}
