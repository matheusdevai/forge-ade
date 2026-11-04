import { CheckCircle2, CircleAlert, Info, Loader2, Mic, Square, TriangleAlert, X } from "lucide-react";
import { Button, Kbd } from "@/components/ui";
import { cn, modKey } from "@/lib/utils";
import { useVoice } from "./store";

const TONE = {
  success: { icon: CheckCircle2, cls: "text-success" },
  warning: { icon: TriangleAlert, cls: "text-warning" },
  error: { icon: CircleAlert, cls: "text-error" },
  neutral: { icon: Info, cls: "text-accent" },
} as const;

/** Floating voice panel: live transcript while listening, then the outcome. */
export function VoiceHud() {
  const { mode, phase, partial, level, result, target, stop, cancel, dismiss } = useVoice();
  if (phase === "idle") return null;
  const listening = phase === "listening" || phase === "starting";
  const title = mode === "clock" ? `Ditando para ${target?.label ?? "o agente"}` : "Jarvis";
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-6 z-[60] flex justify-center px-4">
      <div className="pointer-events-auto w-[min(560px,100%)] rounded-xl border border-border bg-surface/95 shadow-2xl shadow-black/60 backdrop-blur" role="status" aria-live="polite">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2.5">
          <span className={cn("grid size-6 place-items-center rounded-full", listening ? "bg-accent/20 text-accent" : "bg-elevated text-muted")}>
            {phase === "thinking" ? <Loader2 className="size-3.5 animate-spin" /> : <Mic className="size-3.5" />}
          </span>
          <span className="text-[12.5px] font-medium text-fg">{title}</span>
          <span className="text-[11px] text-subtle">
            {phase === "starting" ? "abrindo microfone…" : phase === "listening" ? "ouvindo — pausa encerra" : phase === "thinking" ? "processando…" : ""}
          </span>
          <div className="ml-auto flex items-center gap-1">
            {listening && (
              <Button size="sm" variant="ghost" onClick={stop} title="Parar e processar">
                <Square /> Parar
              </Button>
            )}
            <Button size="icon" variant="ghost" onClick={listening ? cancel : dismiss} aria-label="Fechar">
              <X />
            </Button>
          </div>
        </div>
        <div className="px-4 py-3">
          {listening && (
            <div className="mb-2 flex h-6 items-end gap-[3px]" aria-hidden>
              {Array.from({ length: 24 }).map((_, i) => {
                const h = Math.max(0.12, Math.min(1, level * (0.55 + 0.45 * Math.abs(Math.sin(i * 1.7)))));
                return <span key={i} className="w-[3px] rounded-full bg-accent/80 transition-[height] duration-75" style={{ height: `${h * 100}%` }} />;
              })}
            </div>
          )}
          {(listening || phase === "thinking") && (
            <p className={cn("min-h-5 text-[13.5px] leading-relaxed", partial ? "text-fg" : "text-subtle")}>{partial || (mode === "jarvis" ? "Ex.: “Forge, verifique os erros do frontend”" : "Fale o pedido para o agente…")}</p>
          )}
          {phase === "result" && result && (
            <div>
              <div className="flex items-start gap-2">
                {(() => {
                  const T = TONE[result.tone];
                  return <T.icon className={cn("mt-0.5 size-4 shrink-0", T.cls)} />;
                })()}
                <div className="min-w-0">
                  <p className="text-[13.5px] font-medium text-fg">{result.title}</p>
                  {result.detail && <p className="mt-0.5 text-[12px] leading-relaxed break-words text-muted">{result.detail}</p>}
                </div>
              </div>
              {result.actions && result.actions.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {result.actions.map((a, i) => (
                    <Button key={i} size="sm" variant={a.tone === "danger" ? "danger" : a.tone === "primary" ? "primary" : a.tone === "ghost" ? "ghost" : "secondary"} onClick={() => void a.run()}>
                      {a.label}
                    </Button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
        <div className="flex gap-3 border-t border-border px-4 py-1.5 text-[10.5px] text-subtle">
          <span>
            <Kbd>{modKey}</Kbd>
            <Kbd>⇧</Kbd>
            <Kbd>J</Kbd> Jarvis
          </span>
          <span>
            <Kbd>{modKey}</Kbd>
            <Kbd>⇧</Kbd>
            <Kbd>D</Kbd> ditar para o agente
          </span>
          <span className="ml-auto">Esc fecha</span>
        </div>
      </div>
    </div>
  );
}
