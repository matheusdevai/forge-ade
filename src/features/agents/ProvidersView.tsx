import { useEffect, useState } from "react";
import { BadgeCheck, Bot, ClipboardCopy, ExternalLink, FlaskConical, KeyRound, Loader2, RefreshCw, Terminal } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Badge, Button, Card, Dialog, DialogContent, EmptyState, StatusDot } from "@/components/ui";
import { errorMessage, type ProviderStatus } from "@/lib/ipc";
import { relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useEnvironment } from "@/stores/data";
import { useProviders } from "@/stores/providers";
import { launchAgentPane } from "./actions";
import { AuthBadge, VerifiedBadge } from "./badges";

const CAPS: [keyof ProviderStatus["capabilities"], string][] = [
  ["interactive", "Interativo"],
  ["headless", "Headless"],
  ["structuredOutput", "Saída estruturada"],
  ["resume", "Retomar sessões"],
  ["mcp", "MCP"],
  ["sandbox", "Sandbox"],
];

const PLANNED = [
  { id: "opencode", name: "OpenCode" },
  { id: "ollama", name: "Ollama" },
  { id: "lms", name: "LM Studio" },
  { id: "openrouter", name: "OpenRouter" },
  { id: "deepseek", name: "DeepSeek" },
  { id: "custom", name: "Custom CLI" },
];

const fmt = (n: number | null) => (n === null ? "—" : n.toLocaleString("pt-BR"));

function ProviderCard({ p, onVerify }: { p: ProviderStatus; onVerify: (p: ProviderStatus) => void }) {
  const verifying = useProviders((s) => !!s.verifying[p.id]);
  const route = useApp((s) => s.route);
  const inWorkspace = route.view === "workspace";
  const c = p.lastCheck;

  return (
    <Card className="flex flex-col">
      <div className="flex items-start gap-3 border-b border-border p-4">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-border bg-bg-2 text-accent">
          <Bot className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="text-[14px] font-semibold">{p.name}</h3>
            {p.version && <span className="font-mono text-[11px] text-subtle">v{p.version}</span>}
          </div>
          <div className="mt-1 flex flex-wrap gap-1.5">
            <AuthBadge p={p} />
            {p.installed && <VerifiedBadge p={p} />}
          </div>
        </div>
        <span className="text-[11px] text-subtle">{p.vendor}</span>
      </div>

      <div className="flex-1 space-y-3 p-4 text-[12px]">
        {!p.installed ? (
          <div>
            <p className="text-muted">Não encontrado neste computador. Instale pelo terminal:</p>
            <button type="button" onClick={() => void navigator.clipboard.writeText(p.install).then(() => toast.success("Comando copiado"))} className="mt-1.5 flex w-full items-center gap-2 rounded-md border border-border bg-bg-2 px-2.5 py-1.5 text-left font-mono text-[11.5px] text-fg hover:border-border-strong">
              <span className="flex-1 truncate">{p.install}</span>
              <ClipboardCopy className="size-3.5 text-subtle" />
            </button>
          </div>
        ) : (
          <>
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
              <dt className="text-subtle">Binário</dt>
              <dd className="truncate font-mono text-[11px] text-muted" title={p.path ?? ""}>
                {p.path}
              </dd>
              <dt className="text-subtle">Modelo</dt>
              <dd className="text-muted">
                {p.configuredModel ? <span className="font-mono">{p.configuredModel}</span> : "padrão do CLI"}
                {p.modelAliases.length > 0 && <span className="text-subtle"> · aliases: {p.modelAliases.join(", ")}</span>}
              </dd>
              <dt className="text-subtle">Login</dt>
              <dd className="text-muted">{p.auth.state === "logged_in" ? "autenticação nativa do CLI" : (p.auth.detail ?? p.loginHint)}</dd>
            </dl>
            <div className="flex flex-wrap gap-1">
              {CAPS.map(([k, label]) => (p.capabilities[k] ? <Badge key={k}>{label}</Badge> : null))}
            </div>
          </>
        )}

        {c && (
          <div className="rounded-md border border-border bg-bg-2 p-2.5">
            <div className="flex items-center gap-1.5">
              <StatusDot tone={c.ok ? "success" : "error"} />
              <span className="font-medium text-fg">{c.ok ? "Último teste bem-sucedido" : "Último teste falhou"}</span>
              <span className="ml-auto text-[11px] text-subtle">{relativeTime(c.checkedAt)}</span>
            </div>
            <p className="mt-1 line-clamp-3 text-[11.5px] text-muted">{c.message}</p>
            <div className="mt-1.5 grid grid-cols-2 gap-x-3 font-mono text-[10.5px] text-subtle">
              <span>v{c.version ?? "?"}</span>
              <span>{c.model ?? "modelo n/d"}</span>
              <span>{(c.durationMs / 1000).toFixed(1)} s</span>
              <span>
                tokens {fmt(c.inputTokens)} → {fmt(c.outputTokens)}
              </span>
              {c.costUsd !== null && (
                <span className="col-span-2" title="Valor informado pelo próprio CLI. No Claude Code com assinatura, é o equivalente de API, não uma cobrança à parte.">
                  custo informado pelo CLI: US$ {c.costUsd.toFixed(4)}
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      {p.installed && (
        <div className="flex flex-wrap gap-1.5 border-t border-border p-3">
          <Button size="sm" variant="ghost" disabled={verifying} onClick={() => onVerify(p)}>
            {verifying ? <Loader2 className="animate-spin" /> : <FlaskConical />} Testar conexão
          </Button>
          {p.auth.state !== "logged_in" && (
            <Button
              size="sm"
              variant="secondary"
              disabled={!inWorkspace}
              title={inWorkspace ? undefined : "Abra um workspace para fazer login num pane"}
              onClick={() => launchAgentPane(p.id, "login")}
            >
              <KeyRound /> Fazer login
            </Button>
          )}
          <Button size="sm" variant={p.auth.state === "logged_in" ? "secondary" : "ghost"} disabled={!inWorkspace} title={inWorkspace ? undefined : "Abra um workspace"} onClick={() => launchAgentPane(p.id, "agent")}>
            <Terminal /> Abrir pane
          </Button>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={() => void openUrl(p.docs).catch(() => undefined)}>
            <ExternalLink /> Docs
          </Button>
        </div>
      )}
    </Card>
  );
}

export function ProvidersView() {
  const { items, loaded, loading, refresh, verify } = useProviders();
  const tools = useEnvironment((s) => s.tools);
  const [confirm, setConfirm] = useState<ProviderStatus | null>(null);

  useEffect(() => {
    void refresh().catch((e) => toast.error("Falha ao detectar providers", { description: errorMessage(e) }));
  }, [refresh]);

  const runVerify = async (p: ProviderStatus) => {
    try {
      const c = await verify(p.id);
      if (c.ok) toast.success(`${p.name} verificado`, { description: c.message });
      else toast.error(`${p.name}: teste falhou`, { description: c.message, duration: 10000 });
    } catch (e) {
      toast.error(`${p.name}: teste falhou`, { description: errorMessage(e) });
    }
  };

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-6 flex items-end gap-4">
          <div>
            <h1 className="text-xl font-semibold">Providers</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Agentes de código instalados neste computador, usando a autenticação nativa de cada CLI. Você não paga de novo pelos modelos, e o FORGE nunca vê suas credenciais.
              Um provider fica <b className="text-fg">verificado</b> depois de um teste real bem-sucedido na versão instalada.
            </p>
          </div>
          <Button size="sm" variant="ghost" className="ml-auto" disabled={loading} onClick={() => void refresh()}>
            {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />} Detectar novamente
          </Button>
        </div>

        {!loaded ? (
          <div className="flex justify-center py-16 text-subtle">
            <Loader2 className="size-5 animate-spin" />
          </div>
        ) : (
          <div className="grid gap-4 lg:grid-cols-3">
            {items.map((p) => (
              <ProviderCard key={p.id} p={p} onVerify={setConfirm} />
            ))}
          </div>
        )}

        <h2 className="mt-10 mb-3 text-[10.5px] font-medium uppercase tracking-wider text-subtle">Integrações planejadas</h2>
        <Card>
          {PLANNED.length === 0 ? (
            <EmptyState title="Nada planejado" />
          ) : (
            <ul className="divide-y divide-border">
              {PLANNED.map((x) => {
                const t = tools.find((t) => t.id === x.id);
                return (
                  <li key={x.id} className="flex items-center gap-3 px-4 py-2.5 text-[12.5px]">
                    <StatusDot tone={t?.installed ? "success" : "neutral"} />
                    <span>{x.name}</span>
                    <span className="text-[11px] text-subtle">{t ? (t.installed ? `instalado${t.version ? ` · v${t.version}` : ""}` : "não instalado") : "via API"}</span>
                    <Badge className="ml-auto">ainda não integrado</Badge>
                  </li>
                );
              })}
            </ul>
          )}
        </Card>
        <p className="mt-2 text-[11px] text-subtle">O FORGE só marca uma integração como suportada depois de implementá-la e testá-la.</p>
      </div>

      <Dialog open={!!confirm} onOpenChange={(o) => !o && setConfirm(null)}>
        {confirm && (
          <DialogContent title={`Testar ${confirm.name}?`} description="Teste real de ponta a ponta">
            <div className="px-5 py-4 text-[13px] text-muted">
              O FORGE enviará um prompt mínimo (<code className="font-mono text-fg">FORGE_OK</code>) pelo {confirm.name} em modo somente leitura, usando o seu login. Isso consome uma pequena quantidade do seu plano ou créditos.
              <div className="mt-4 flex justify-end gap-2">
                <Button variant="ghost" onClick={() => setConfirm(null)}>
                  Cancelar
                </Button>
                <Button
                  variant="primary"
                  onClick={() => {
                    const p = confirm;
                    setConfirm(null);
                    void runVerify(p);
                  }}
                >
                  <BadgeCheck /> Testar agora
                </Button>
              </div>
            </div>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
