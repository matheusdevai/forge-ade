import { lazy, Suspense, useEffect, useState } from "react";
import { ClipboardCopy, KeyRound, Loader2, Play, RotateCcw, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import { Button, EmptyState, Tooltip } from "@/components/ui";
import { AuthBadge, VerifiedBadge } from "./badges";
import { findPane, setPaneState } from "@/features/panes/layout";
import type { PaneProps } from "@/features/panes/registry";
import { useLayout } from "@/features/panes/store";
import type { ProviderId } from "@/lib/ipc";
import { useProviders } from "@/stores/providers";

const TerminalPane = lazy(() => import("@/features/panes/kinds/TerminalPane").then((m) => ({ default: m.TerminalPane })));

const copy = (text: string) =>
  navigator.clipboard
    .writeText(text)
    .then(() => toast.success("Copiado", { description: text }))
    .catch(() => toast.error("Não foi possível copiar"));

export function AgentPane({ pane, workspaceId }: PaneProps) {
  const providerId = pane.kind as ProviderId;
  const provider = useProviders((s) => s.items.find((p) => p.id === providerId));
  const loaded = useProviders((s) => s.loaded);
  const refresh = useProviders((s) => s.refresh);
  const refreshOne = useProviders((s) => s.refreshOne);
  const apply = useLayout((s) => s.apply);

  const mode: "agent" | "login" = pane.state?.launchMode === "login" ? "login" : "agent";
  const model = typeof pane.state?.model === "string" && pane.state.model ? pane.state.model : null;
  const nonce = typeof pane.state?.launchNonce === "number" ? pane.state.launchNonce : 0;
  const [modelInput, setModelInput] = useState(model ?? "");

  useEffect(() => {
    if (!loaded) void refresh();
  }, [loaded, refresh]);
  useEffect(() => setModelInput(model ?? ""), [model]);

  const update = (patch: Record<string, unknown>, restart: boolean) =>
    apply((d) => {
      const p = findPane(d, pane.id);
      if (!p) return d;
      const state = p.state ?? {};
      const n = typeof state.launchNonce === "number" ? state.launchNonce : 0;
      return setPaneState(d, pane.id, { ...state, ...patch, ...(restart ? { launchNonce: n + 1 } : {}) });
    });

  if (!provider) {
    return (
      <div className="flex h-full items-center justify-center text-subtle">
        <Loader2 className="size-4 animate-spin" />
      </div>
    );
  }

  if (!provider.installed) {
    return (
      <EmptyState
        className="h-full"
        icon={<ShieldAlert />}
        title={`${provider.name} não está instalado`}
        description={<>Instale pelo terminal e depois verifique novamente: <code className="font-mono text-fg">{provider.install}</code></>}
        action={
          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={() => copy(provider.install)}>
              <ClipboardCopy /> Copiar comando
            </Button>
            <Button size="sm" onClick={() => void refreshOne(provider.id)}>
              Verificar novamente
            </Button>
          </div>
        }
      />
    );
  }

  const applyModel = () => {
    const m = modelInput.trim();
    if (m && !/^[A-Za-z0-9._:/-]{1,100}$/.test(m)) {
      toast.error("Nome de modelo inválido");
      return;
    }
    update({ model: m || null }, true);
  };

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border bg-bg px-2.5 text-[11.5px]">
        <span className="font-medium text-fg">{provider.name}</span>
        {provider.version && <span className="font-mono text-subtle">v{provider.version}</span>}
        <AuthBadge p={provider} />
        <VerifiedBadge p={provider} />
        <div className="ml-auto flex items-center gap-1">
          {mode === "agent" && (
            <>
              <input
                list={`models-${pane.id}`}
                value={modelInput}
                onChange={(e) => setModelInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && applyModel()}
                placeholder={provider.configuredModel ? `padrão: ${provider.configuredModel}` : "modelo padrão do CLI"}
                aria-label="Modelo"
                className="h-6 w-40 rounded border border-border bg-bg-2 px-2 font-mono text-[11px] text-fg placeholder:text-subtle focus:border-accent/60 focus:outline-none"
              />
              <datalist id={`models-${pane.id}`}>
                {provider.modelAliases.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
              {(modelInput.trim() || null) !== model && (
                <Button size="sm" variant="ghost" className="h-6 px-2" onClick={applyModel}>
                  Aplicar
                </Button>
              )}
            </>
          )}
          {provider.auth.state !== "logged_in" && mode === "agent" && (
            <Button size="sm" variant="secondary" className="h-6 px-2" onClick={() => update({ launchMode: "login" }, true)}>
              <KeyRound /> Fazer login
            </Button>
          )}
          {mode === "login" && (
            <Button
              size="sm"
              variant="primary"
              className="h-6 px-2"
              onClick={() => {
                update({ launchMode: "agent" }, true);
                void refreshOne(provider.id);
              }}
            >
              <Play /> Iniciar agente
            </Button>
          )}
          <Tooltip content="Reiniciar sessão">
            <Button size="icon" variant="ghost" className="size-6" aria-label="Reiniciar sessão" onClick={() => update({}, true)}>
              <RotateCcw />
            </Button>
          </Tooltip>
        </div>
      </div>
      {mode === "login" && (
        <div className="shrink-0 border-b border-border bg-accent/10 px-3 py-1.5 text-[11.5px] text-accent">
          Sessão de login de {provider.name}: siga as instruções abaixo (pode abrir o navegador). O FORGE não vê suas credenciais. Ao terminar, clique em “Iniciar agente”.
        </div>
      )}
      <div className="min-h-0 flex-1">
        <Suspense fallback={null}>
          <TerminalPane pane={pane} workspaceId={workspaceId} launch={{ providerId: provider.id, mode, model }} restartSignal={nonce} />
        </Suspense>
      </div>
    </div>
  );
}
