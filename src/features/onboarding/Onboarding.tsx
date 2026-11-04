import { useState } from "react";
import { ArrowRight, FolderOpen } from "lucide-react";
import { Logo, Mascot, hasMascot } from "@/brand";
import { Button } from "@/components/ui";
import { EnvironmentTable } from "@/features/environment/EnvironmentTable";
import { pickAndOpenWorkspace } from "@/features/workspace/actions";
import { cn } from "@/lib/utils";
import { useEnvironment } from "@/stores/data";
import { useSettings } from "@/stores/settings";

const STEPS = ["Boas-vindas", "Ambiente", "Primeiro workspace"] as const;

export function Onboarding() {
  const [step, setStep] = useState(0);
  const setSetting = useSettings((s) => s.set);
  const { tools, scanning } = useEnvironment();
  const missingRequired = tools.filter((t) => t.required && !t.installed);

  const finish = async (openWs: boolean) => {
    await setSetting("onboarding.completed", true);
    if (openWs) await pickAndOpenWorkspace();
  };

  return (
    <div className="flex h-full items-center justify-center bg-[radial-gradient(80%_60%_at_50%_0%,#579bff14,transparent)] px-6" data-tauri-drag-region>
      <div className="w-full max-w-2xl">
        <div className="mb-6 flex items-center justify-center gap-2">
          {STEPS.map((s, i) => (
            <div key={s} className={cn("h-1 w-10 rounded-full transition-colors", i <= step ? "bg-accent" : "bg-border")} title={s} />
          ))}
        </div>

        <div className="rounded-2xl border border-border bg-surface p-8 shadow-2xl shadow-black/40">
          {step === 0 && (
            <div className="flex flex-col items-center text-center">
              {hasMascot && <Mascot state="online" size={168} />}
              <Logo className={cn("h-8", hasMascot && "mt-4")} />
              <p className="mt-2 text-[11px] font-medium uppercase tracking-[0.3em] text-subtle">Build beyond limits</p>
              <h1 className="mt-6 text-xl font-semibold">Bem-vindo ao seu Agentic Development Environment</h1>
              <p className="mt-2 max-w-md text-[13px] leading-relaxed text-muted">
                Comande, coordene e acompanhe equipes de agentes de IA trabalhando em projetos reais. Vamos verificar seu ambiente.
              </p>
              <Button variant="primary" size="lg" className="mt-8" onClick={() => setStep(1)}>
                Começar <ArrowRight />
              </Button>
            </div>
          )}

          {step === 1 && (
            <div>
              <div className="mb-5 flex items-center gap-4">
                {hasMascot && <Mascot state={scanning ? "thinking" : missingRequired.length ? "error" : "success"} size={56} />}
                <div>
                  <h2 className="text-lg font-semibold">Verificação do ambiente</h2>
                  <p className="text-xs text-muted">O FORGE detecta as ferramentas instaladas — nada é configurado sem você.</p>
                </div>
              </div>
              <div className="max-h-[46vh] overflow-auto pr-1">
                <EnvironmentTable compact />
              </div>
              {missingRequired.length > 0 && (
                <p className="mt-3 text-xs text-warning">
                  Ferramentas obrigatórias ausentes: {missingRequired.map((t) => t.name).join(", ")}. Você pode continuar e instalar depois.
                </p>
              )}
              <div className="mt-6 flex justify-between">
                <Button variant="ghost" onClick={() => setStep(0)}>
                  Voltar
                </Button>
                <Button variant="primary" onClick={() => setStep(2)} disabled={scanning}>
                  Continuar <ArrowRight />
                </Button>
              </div>
            </div>
          )}

          {step === 2 && (
            <div className="flex flex-col items-center text-center">
              {hasMascot && <Mascot state="waiting" size={96} />}
              <h2 className="mt-4 text-lg font-semibold">Abra seu primeiro workspace</h2>
              <p className="mt-2 max-w-md text-[13px] leading-relaxed text-muted">
                Um workspace é uma pasta de projeto com seus próprios panes, missões e histórico. Os arquivos permanecem onde estão.
              </p>
              <div className="mt-8 flex gap-2">
                <Button variant="ghost" onClick={() => void finish(false)}>
                  Pular por agora
                </Button>
                <Button variant="primary" size="lg" onClick={() => void finish(true)}>
                  <FolderOpen /> OPEN WORKSPACE
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
