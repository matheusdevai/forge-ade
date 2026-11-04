import { useCallback, useEffect, useState } from "react";
import { CheckCircle2, ChevronRight, CircleDashed, KeyRound, LayoutTemplate, Loader2, Plug, Plus, Save, ShieldCheck, Trash2, TriangleAlert, Zap } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Badge, Button, Card, CardHeader, Dialog, DialogContent, EmptyState, Input, Label, StatusDot, Switch, Textarea } from "@/components/ui";
import { api, errorMessage, type McpDetail, type McpProbe, type McpServer, type McpServerInput, type McpTemplate } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useAgents } from "@/stores/agents";
import { useLibrary } from "@/stores/library";

const toInput = (s: McpServer): McpServerInput => ({
  label: s.label,
  transport: s.transport,
  command: s.command,
  args: s.args,
  env: s.env,
  url: s.url,
  headers: s.headers,
  secretKeys: s.secretKeys,
  readOnly: s.readOnly,
  enabled: s.enabled,
});

const kvToText = (m: Record<string, string>, sep: string) =>
  Object.entries(m)
    .map(([k, v]) => `${k}${sep}${v}`)
    .join("\n");
const textToKv = (t: string, sep: string) =>
  Object.fromEntries(
    t
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean)
      .map((l) => {
        const i = l.indexOf(sep);
        return i < 0 ? [l, ""] : [l.slice(0, i).trim(), l.slice(i + sep.length).trim()];
      }),
  );

function StatusBadge({ s }: { s: McpServer }) {
  if (s.lastOk === true) return <Badge tone="success">verificado · {s.tools.length} ferramentas</Badge>;
  if (s.lastOk === false) return <Badge tone="error">falhou</Badge>;
  return <Badge>não verificado</Badge>;
}

function Secrets({ detail, onChange }: { detail: McpDetail; onChange: () => void }) {
  const [editing, setEditing] = useState<string | null>(null);
  const [value, setValue] = useState("");
  if (detail.server.secretKeys.length === 0) return <p className="text-[12px] text-subtle">Este servidor não declara segredos.</p>;
  return (
    <div className="space-y-1.5">
      {detail.server.secretKeys.map((k) => {
        const set = detail.secretStatus[k];
        return (
          <div key={k} className="flex items-center gap-2 text-[12.5px]">
            <KeyRound className={cn("size-3.5", set ? "text-success" : "text-warning")} />
            <span className="font-mono">{k}</span>
            <span className="text-[11px] text-subtle">{set ? "definido no cofre do sistema" : "não definido"}</span>
            <span className="ml-auto flex gap-1">
              <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => { setEditing(k); setValue(""); }}>
                {set ? "Substituir" : "Definir"}
              </Button>
              {set && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-6 px-2"
                  onClick={() =>
                    void api
                      .mcpClearSecret(detail.server.id, k)
                      .then(() => {
                        toast.success("Segredo removido do cofre");
                        onChange();
                      })
                      .catch((e) => toast.error("Falha", { description: errorMessage(e) }))
                  }
                >
                  Remover
                </Button>
              )}
            </span>
          </div>
        );
      })}
      <Dialog open={!!editing} onOpenChange={(o) => !o && setEditing(null)}>
        <DialogContent title={`Definir ${editing}`} description="O valor vai direto para o cofre do sistema (Keychain). O FORGE nunca o exibe nem grava em banco ou logs.">
          <form
            className="space-y-3 px-5 py-4"
            onSubmit={(e) => {
              e.preventDefault();
              const key = editing!;
              void api
                .mcpSetSecret(detail.server.id, key, value)
                .then(() => {
                  setEditing(null);
                  setValue("");
                  toast.success("Segredo guardado no cofre");
                  onChange();
                })
                .catch((err) => toast.error("Falha ao guardar", { description: errorMessage(err) }));
            }}
          >
            <Input type="password" autoFocus autoComplete="off" value={value} onChange={(e) => setValue(e.target.value)} aria-label="Valor do segredo" />
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setEditing(null)}>
                Cancelar
              </Button>
              <Button type="submit" variant="primary" disabled={!value}>
                Guardar
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function Associations({ detail, onChange }: { detail: McpDetail; onChange: () => void }) {
  const profiles = useAgents((s) => s.profiles);
  const squads = useLibrary((s) => s.squads);
  const linked = (kind: string, id: string) => detail.links.some(([k, t]) => k === kind && t === id);
  const toggle = (kind: "profile" | "squad", id: string, on: boolean) =>
    void api
      .mcpSetLink(detail.server.id, kind, id, on)
      .then(onChange)
      .catch((e) => toast.error("Falha", { description: errorMessage(e) }));
  return (
    <div className="grid gap-4 md:grid-cols-2">
      {(
        [
          ["profile", "Agentes", profiles],
          ["squad", "Squads", squads],
        ] as const
      ).map(([kind, title, items]) => (
        <div key={kind}>
          <div className="mb-1.5 text-[10.5px] uppercase tracking-wider text-subtle">{title}</div>
          <div className="max-h-52 space-y-1 overflow-auto">
            {items.map((it) => (
              <label key={it.id} className="flex items-center gap-2 text-[12.5px] text-muted">
                <input type="checkbox" className="accent-[#579bff]" checked={linked(kind, it.id)} onChange={(e) => toggle(kind, it.id, e.target.checked)} />
                {it.name}
              </label>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function Editor({ id, onChanged, onDeleted }: { id: string; onChanged: () => void; onDeleted: () => void }) {
  const [detail, setDetail] = useState<McpDetail | null>(null);
  const [form, setForm] = useState<McpServerInput | null>(null);
  const [argsText, setArgsText] = useState("");
  const [envText, setEnvText] = useState("");
  const [headersText, setHeadersText] = useState("");
  const [secretsText, setSecretsText] = useState("");
  const [testing, setTesting] = useState(false);
  const [probe, setProbe] = useState<McpProbe | null>(null);
  const [openTool, setOpenTool] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const load = useCallback(() => {
    api
      .mcpDetail(id)
      .then((d) => {
        setDetail(d);
        const f = toInput(d.server);
        setForm(f);
        setArgsText(f.args.join("\n"));
        setEnvText(kvToText(f.env, "="));
        setHeadersText(kvToText(f.headers, ": "));
        setSecretsText(f.secretKeys.join(", "));
      })
      .catch((e) => toast.error("Falha ao carregar o servidor", { description: errorMessage(e) }));
  }, [id]);
  useEffect(load, [load]);

  if (!detail || !form) return null;
  const s = detail.server;
  const draft: McpServerInput = {
    ...form,
    args: argsText.split("\n").map((a) => a.trim()).filter(Boolean),
    env: textToKv(envText, "="),
    headers: textToKv(headersText, ":"),
    secretKeys: secretsText.split(/[,\s]+/).map((k) => k.trim()).filter(Boolean),
  };
  const dirty = JSON.stringify(draft) !== JSON.stringify(toInput(s));
  const tools = probe?.ok ? probe.tools : s.tools;
  const log = probe?.log ?? s.lastLog;

  const test = async () => {
    setTesting(true);
    try {
      const r = await api.mcpTest(s.id);
      setProbe(r);
      if (r.ok) toast.success(`Conectado: ${r.tools.length} ferramenta(s)`);
      else toast.error("Falha na conexão", { description: r.error ?? undefined, duration: 9000 });
      load();
      onChanged();
    } catch (e) {
      toast.error("Não foi possível testar", { description: errorMessage(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-[15px] font-semibold">{s.label}</h2>
        <Badge>{s.transport}</Badge>
        <Badge className="font-mono">{s.name}</Badge>
        <StatusBadge s={s} />
        {s.readOnly && <Badge tone="accent">somente leitura</Badge>}
        <label className="ml-auto flex items-center gap-2 text-[12px] text-muted">
          <Switch
            label="Ativo"
            checked={s.enabled}
            onCheckedChange={(v) =>
              void api
                .mcpSetEnabled(s.id, v)
                .then(() => {
                  load();
                  onChanged();
                })
                .catch((e) => toast.error("Falha", { description: errorMessage(e) }))
            }
          />
          {s.enabled ? "ativo" : "desativado"}
        </label>
      </div>
      {s.lastCheckAt && (
        <p className="text-[11.5px] text-subtle">
          Último teste {relativeTime(s.lastCheckAt)}
          {s.serverInfo?.serverInfo?.name ? ` · ${s.serverInfo.serverInfo.name} ${s.serverInfo.serverInfo.version ?? ""}` : ""}
          {s.serverInfo?.protocolVersion ? ` · protocolo ${s.serverInfo.protocolVersion}` : ""}
        </p>
      )}
      {s.lastOk === false && s.lastError && (
        <p className="flex items-start gap-2 rounded-md border border-error/30 bg-error/10 p-2 text-[12px] text-error">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" /> {s.lastError}
        </p>
      )}

      <div className="grid grid-cols-[1fr_160px] gap-3">
        <div>
          <Label htmlFor="mcp-label">Nome</Label>
          <Input id="mcp-label" value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} />
        </div>
        <div>
          <Label htmlFor="mcp-transport">Transporte</Label>
          <select
            id="mcp-transport"
            value={form.transport}
            onChange={(e) => setForm({ ...form, transport: e.target.value as "stdio" | "http" })}
            className="h-9 w-full rounded-md border border-border bg-bg-2 px-2 text-[13px] text-fg"
          >
            <option value="stdio">stdio (local)</option>
            <option value="http">HTTP (remoto)</option>
          </select>
        </div>
      </div>
      {form.transport === "stdio" ? (
        <div className="grid gap-3 md:grid-cols-2">
          <div>
            <Label htmlFor="mcp-cmd">Comando</Label>
            <Input id="mcp-cmd" className="font-mono text-[12px]" value={form.command ?? ""} onChange={(e) => setForm({ ...form, command: e.target.value })} placeholder="npx" />
            <Label htmlFor="mcp-args" className="mt-3">
              Argumentos (um por linha)
            </Label>
            <Textarea id="mcp-args" rows={4} className="font-mono text-[12px]" value={argsText} onChange={(e) => setArgsText(e.target.value)} />
          </div>
          <div>
            <Label htmlFor="mcp-env">Variáveis de ambiente (CHAVE=valor, não secretas)</Label>
            <Textarea id="mcp-env" rows={7} className="font-mono text-[12px]" value={envText} onChange={(e) => setEnvText(e.target.value)} />
          </div>
        </div>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          <div>
            <Label htmlFor="mcp-url">URL</Label>
            <Input id="mcp-url" className="font-mono text-[12px]" value={form.url ?? ""} onChange={(e) => setForm({ ...form, url: e.target.value })} placeholder="https://…/mcp" />
          </div>
          <div>
            <Label htmlFor="mcp-headers">Cabeçalhos (Nome: valor)</Label>
            <Textarea id="mcp-headers" rows={3} className="font-mono text-[12px]" value={headersText} onChange={(e) => setHeadersText(e.target.value)} placeholder="Authorization: Bearer ${TOKEN}" />
          </div>
        </div>
      )}
      <div className="grid gap-3 md:grid-cols-2">
        <div>
          <Label htmlFor="mcp-secrets">Nomes dos segredos (use ${"{NOME}"} nos campos acima)</Label>
          <Input id="mcp-secrets" className="font-mono text-[12px]" value={secretsText} onChange={(e) => setSecretsText(e.target.value)} placeholder="API_TOKEN, DATABASE_URL" />
          <p className="mt-1 text-[11px] text-subtle">Em servidores stdio, os segredos também são passados como variáveis de ambiente.</p>
        </div>
        <label className="flex items-start gap-2 pt-6 text-[12.5px] text-muted">
          <input type="checkbox" className="mt-0.5 accent-[#579bff]" checked={form.readOnly} onChange={(e) => setForm({ ...form, readOnly: e.target.checked })} />
          <span>
            Servidor somente leitura
            <span className="block text-[11px] text-subtle">Permite que agentes em modo leitura usem as ferramentas. Marque só se o servidor não altera nada.</span>
          </span>
        </label>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="ghost" onClick={() => setConfirmDelete(true)}>
          <Trash2 /> Remover
        </Button>
        <Button variant="ghost" disabled={testing || dirty} title={dirty ? "Salve antes de testar" : undefined} onClick={() => void test()}>
          {testing ? <Loader2 className="animate-spin" /> : <Zap />} Testar conexão
        </Button>
        <Button
          variant="primary"
          className="ml-auto"
          disabled={!dirty}
          onClick={() =>
            void api
              .mcpUpdate(s.id, draft)
              .then(() => {
                toast.success("Servidor salvo");
                load();
                onChanged();
              })
              .catch((e) => toast.error("Configuração inválida", { description: errorMessage(e) }))
          }
        >
          <Save /> Salvar
        </Button>
      </div>

      <Card>
        <CardHeader title="Segredos" icon={<ShieldCheck />} />
        <div className="p-4">
          <Secrets detail={detail} onChange={load} />
        </div>
      </Card>

      <Card>
        <CardHeader title={`Ferramentas (${tools.length})`} icon={<Plug />} />
        {tools.length === 0 ? (
          <p className="p-4 text-[12px] text-subtle">Teste a conexão para listar as ferramentas oferecidas pelo servidor.</p>
        ) : (
          <ul className="max-h-80 overflow-auto p-2">
            {tools.map((t) => (
              <li key={t.name}>
                <button type="button" onClick={() => setOpenTool(openTool === t.name ? null : t.name)} className="flex w-full items-start gap-2 rounded px-2 py-1.5 text-left hover:bg-elevated">
                  <ChevronRight className={cn("mt-0.5 size-3 shrink-0 text-subtle transition-transform", openTool === t.name && "rotate-90")} />
                  <span className="shrink-0 font-mono text-[12px] text-fg">{t.name}</span>
                  <span className="truncate text-[11.5px] text-subtle">{t.description}</span>
                </button>
                {openTool === t.name && (
                  <pre className="selectable mx-2 mb-2 max-h-48 overflow-auto rounded bg-bg-2 p-2 font-mono text-[11px] whitespace-pre-wrap text-muted">
                    {t.description}
                    {"\n\n"}
                    {JSON.stringify(t.inputSchema, null, 2)}
                  </pre>
                )}
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card>
        <CardHeader title="Associações" />
        <div className="p-4">
          <Associations detail={detail} onChange={load} />
          <p className="mt-3 text-[11px] leading-relaxed text-subtle">
            Servidores ativos ligados ao agente, ao squad ou à missão são anexados às execuções do <b className="text-fg">Claude Code</b> (somente eles, via --strict-mcp-config). Codex e Gemini: anexar por execução ainda não é suportado (planejado).
          </p>
        </div>
      </Card>

      {log && (
        <Card>
          <CardHeader title="Log do último teste" />
          <pre className="selectable max-h-60 overflow-auto p-4 font-mono text-[11px] whitespace-pre-wrap text-muted">{log}</pre>
        </Card>
      )}

      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent title="Remover servidor MCP?" description={s.label}>
          <div className="px-5 py-4 text-[13px] text-muted">
            O servidor, suas associações e seus segredos no cofre do sistema serão removidos.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirmDelete(false);
                  void api
                    .mcpDelete(s.id)
                    .then(() => {
                      toast.success("Servidor removido");
                      onDeleted();
                    })
                    .catch((e) => toast.error("Falha", { description: errorMessage(e) }));
                }}
              >
                Remover
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function TemplatesDialog({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: (s: McpServer) => void }) {
  const [templates, setTemplates] = useState<McpTemplate[]>([]);
  const [picked, setPicked] = useState<McpTemplate | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setPicked(null);
      setValues({});
      void api.mcpTemplates().then(setTemplates).catch(() => setTemplates([]));
    }
  }, [open]);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={picked ? picked.label : "Modelos de servidor MCP"} description={picked?.description ?? "Integrações prioritárias. Cada uma só aparece como verificada depois de um teste real nesta máquina."} className="w-[min(760px,calc(100vw-32px))]">
        {!picked ? (
          <div className="grid max-h-[62vh] gap-2 overflow-auto px-5 py-4 md:grid-cols-2">
            {templates.map((t) => (
              <button key={t.id} type="button" onClick={() => setPicked(t)} className="rounded-lg border border-border bg-bg-2 p-3 text-left hover:border-border-strong">
                <div className="flex items-center gap-2">
                  <span className="text-[13px] font-medium text-fg">{t.label}</span>
                  <Badge>{t.transport}</Badge>
                  {t.readOnly && <Badge tone="accent">leitura</Badge>}
                </div>
                <p className="mt-1 text-[11.5px] text-muted">{t.description}</p>
                <p className="mt-1 text-[11px] text-subtle">{t.requirements}</p>
              </button>
            ))}
          </div>
        ) : (
          <form
            className="space-y-3 px-5 py-4"
            onSubmit={(e) => {
              e.preventDefault();
              setBusy(true);
              void api
                .mcpCreateFromTemplate(picked.id, null, values)
                .then((s) => {
                  toast.success("Servidor criado — teste a conexão");
                  onCreated(s);
                })
                .catch((err) => toast.error("Não foi possível criar", { description: errorMessage(err) }))
                .finally(() => setBusy(false));
            }}
          >
            <p className="text-[12px] text-muted">{picked.requirements}</p>
            <pre className="overflow-auto rounded-md bg-bg-2 p-2 font-mono text-[11.5px] text-muted">
              {picked.transport === "stdio" ? `${picked.command} ${picked.args.join(" ")}` : picked.url}
            </pre>
            {picked.inputs.map((i) => (
              <div key={i.key}>
                <Label htmlFor={`tpl-${i.key}`}>
                  {i.label} {i.secret && <span className="text-[11px] text-subtle">(guardado no cofre do sistema)</span>}
                </Label>
                <Input id={`tpl-${i.key}`} type={i.secret ? "password" : "text"} autoComplete="off" placeholder={i.placeholder} value={values[i.key] ?? ""} onChange={(e) => setValues({ ...values, [i.key]: e.target.value })} />
              </div>
            ))}
            <div className="flex items-center justify-end gap-2">
              <button type="button" className="mr-auto text-[12px] text-accent hover:underline" onClick={() => void openUrl(picked.docs).catch(() => undefined)}>
                Documentação
              </button>
              <Button type="button" variant="ghost" onClick={() => setPicked(null)}>
                Voltar
              </Button>
              <Button type="submit" variant="primary" disabled={busy || picked.inputs.some((i) => !values[i.key]?.trim())}>
                Criar servidor
              </Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function McpView({ id }: { id?: string }) {
  const navigate = useApp((s) => s.navigate);
  const [servers, setServers] = useState<McpServer[] | null>(null);
  const [templatesOpen, setTemplatesOpen] = useState(false);
  const agentsLoaded = useAgents((s) => s.loaded);
  const libLoaded = useLibrary((s) => s.loaded);
  const load = useCallback(() => {
    api.mcpList().then(setServers).catch((e) => toast.error("Falha ao carregar MCP", { description: errorMessage(e) }));
  }, []);
  useEffect(() => {
    load();
    if (!agentsLoaded) void useAgents.getState().refresh().catch(() => undefined);
    if (!libLoaded) void useLibrary.getState().refresh().catch(() => undefined);
  }, [load, agentsLoaded, libLoaded]);
  useEffect(() => {
    if (!id && servers?.[0]) navigate({ view: "mcp", id: servers[0].id });
  }, [id, servers, navigate]);

  const createBlank = () =>
    void api
      .mcpCreate({ label: "Novo servidor", transport: "stdio", command: "npx", args: [], env: {}, url: null, headers: {}, secretKeys: [], readOnly: false, enabled: true })
      .then((s) => {
        load();
        navigate({ view: "mcp", id: s.id });
      })
      .catch((e) => toast.error("Falha", { description: errorMessage(e) }));

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-5 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">MCP</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Servidores Model Context Protocol (stdio ou HTTP). Segredos ficam no cofre do sistema. Anexe servidores a agentes, squads ou missões para que o Claude Code os use.
            </p>
          </div>
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" onClick={createBlank}>
              <Plus /> Servidor manual
            </Button>
            <Button variant="primary" onClick={() => setTemplatesOpen(true)}>
              <LayoutTemplate /> Adicionar de modelo
            </Button>
          </div>
        </div>
        {!servers ? null : servers.length === 0 ? (
          <Card>
            <EmptyState
              icon={<Plug />}
              title="Nenhum servidor MCP"
              description="Adicione a partir de um modelo (Filesystem, GitHub, Browser, Supabase, PostgreSQL, Docker, Vercel, Cloudflare) ou configure manualmente."
              action={
                <Button variant="primary" onClick={() => setTemplatesOpen(true)}>
                  <LayoutTemplate /> Adicionar de modelo
                </Button>
              }
            />
          </Card>
        ) : (
          <div className="grid gap-4 lg:grid-cols-[280px_1fr]">
            <Card className="h-fit overflow-hidden">
              <ul className="p-1.5">
                {servers.map((s) => (
                  <li key={s.id}>
                    <button
                      type="button"
                      onClick={() => navigate({ view: "mcp", id: s.id })}
                      className={cn("flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left", id === s.id ? "bg-accent/15" : "hover:bg-elevated")}
                    >
                      {s.lastOk === true ? <CheckCircle2 className="size-3.5 text-success" /> : s.lastOk === false ? <TriangleAlert className="size-3.5 text-error" /> : <CircleDashed className="size-3.5 text-subtle" />}
                      <span className={cn("min-w-0 flex-1 truncate text-[12.5px]", s.enabled ? "text-fg" : "text-subtle line-through")}>{s.label}</span>
                      <StatusDot tone={s.enabled ? "success" : "neutral"} />
                      <span className="text-[10.5px] text-subtle">{s.transport}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </Card>
            <Card className="h-fit p-5">{id ? <Editor key={id} id={id} onChanged={load} onDeleted={() => { load(); navigate({ view: "mcp" }); }} /> : null}</Card>
          </div>
        )}
      </div>
      <TemplatesDialog
        open={templatesOpen}
        onClose={() => setTemplatesOpen(false)}
        onCreated={(s) => {
          setTemplatesOpen(false);
          load();
          navigate({ view: "mcp", id: s.id });
        }}
      />
    </div>
  );
}
