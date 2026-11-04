import { useEffect, useState } from "react";
import { CheckCircle2, CircleDashed, Cpu, Mic, Database, Info, LayoutTemplate, Monitor, Palette, Pencil, Settings2, Shapes, Trash2, Wrench } from "lucide-react";
import { toast } from "sonner";
import { brandStatus } from "@/brand";
import { Badge, Button, EmptyState, Input, StatusDot, Switch } from "@/components/ui";
import { useWorkspaces } from "@/stores/data";
import { BUILTIN_PRESETS } from "@/features/panes/presets";
import { usePresets } from "@/features/panes/store";
import { EnvironmentTable } from "@/features/environment/EnvironmentTable";
import { pickDirectory } from "@/features/workspace/actions";
import { api, errorMessage, type AppInfo, type SettingKey, type Settings, type TerminalInfo } from "@/lib/ipc";
import { cn, prettyPath, relativeTime } from "@/lib/utils";
import { useApp, type SettingsSection } from "@/stores/app";
import { useSettings } from "@/stores/settings";
import { VoiceSettings } from "@/features/voice/VoiceSettings";
import { AboutSection } from "./About";

const SECTIONS: { id: SettingsSection; label: string; icon: typeof Settings2 }[] = [
  { id: "general", label: "Geral", icon: Settings2 },
  { id: "appearance", label: "Aparência", icon: Palette },
  { id: "voice", label: "Voz", icon: Mic },
  { id: "layouts", label: "Layouts", icon: LayoutTemplate },
  { id: "processes", label: "Processos", icon: Cpu },
  { id: "environment", label: "Ambiente", icon: Wrench },
  { id: "brand", label: "Identidade visual", icon: Shapes },
  { id: "data", label: "Dados", icon: Database },
  { id: "about", label: "Sobre", icon: Info },
];

function Row({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-6 border-b border-border py-4 last:border-0">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium">{title}</div>
        {description && <div className="mt-0.5 text-xs leading-relaxed text-muted">{description}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function useSetting<K extends SettingKey>(key: K): [Settings[K], (v: Settings[K]) => void] {
  const value = useSettings((s) => s.values[key]);
  const set = useSettings((s) => s.set);
  return [value, (v) => set(key, v).catch((e) => toast.error("Falha ao salvar configuração", { description: errorMessage(e) }))];
}

function General() {
  const [restore, setRestore] = useSetting("session.restore");
  const [dir, setDir] = useSetting("workspace.defaultDir");
  return (
    <>
      <Row title="Restaurar sessão ao iniciar" description="Reabre o último workspace com o layout de panes salvo.">
        <Switch label="Restaurar sessão" checked={restore} onCheckedChange={setRestore} />
      </Row>
      <Row title="Pasta padrão de projetos" description={dir ? prettyPath(dir) : "Usada como ponto de partida ao abrir workspaces."}>
        <div className="flex gap-2">
          {dir && (
            <Button size="sm" variant="ghost" onClick={() => setDir(null)}>
              Limpar
            </Button>
          )}
          <Button
            size="sm"
            onClick={async () => {
              const p = await pickDirectory("Pasta padrão de projetos");
              if (p) setDir(p);
            }}
          >
            Escolher…
          </Button>
        </div>
      </Row>
    </>
  );
}

function Appearance() {
  const [mascot, setMascot] = useSetting("ui.mascot");
  const [reduce, setReduce] = useSetting("ui.reduceMotion");
  return (
    <>
      <Row title="Tema" description="O FORGE é dark-first. Um tema claro poderá ser adicionado depois.">
        <Badge>Dark</Badge>
      </Row>
      <Row title="Mascote" description="Exibe o robô FORGE no onboarding, no dashboard e em estados vazios.">
        <Switch label="Mascote" checked={mascot} onCheckedChange={setMascot} />
      </Row>
      <Row title="Reduzir movimento" description="Desativa animações e transições.">
        <Switch label="Reduzir movimento" checked={reduce} onCheckedChange={setReduce} />
      </Row>
    </>
  );
}

function PresetRow({ id, name }: { id: string; name: string }) {
  const { rename, remove } = usePresets.getState();
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(name);
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="flex items-center gap-2 px-3 py-2">
      <LayoutTemplate className="size-3.5 text-subtle" />
      {editing ? (
        <form
          className="flex flex-1 gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (value.trim()) await rename(id, value.trim());
            setEditing(false);
          }}
        >
          <Input autoFocus className="h-7" maxLength={60} value={value} onChange={(e) => setValue(e.target.value)} />
          <Button size="sm" type="submit">Salvar</Button>
          <Button size="sm" type="button" variant="ghost" onClick={() => setEditing(false)}>Cancelar</Button>
        </form>
      ) : (
        <>
          <span className="flex-1 text-[13px]">{name}</span>
          {confirm ? (
            <>
              <span className="text-xs text-muted">Excluir?</span>
              <Button size="sm" variant="danger" onClick={() => void remove(id)}>Excluir</Button>
              <Button size="sm" variant="ghost" onClick={() => setConfirm(false)}>Não</Button>
            </>
          ) : (
            <>
              <Button size="icon" variant="ghost" aria-label={`Renomear ${name}`} onClick={() => { setValue(name); setEditing(true); }}>
                <Pencil />
              </Button>
              <Button size="icon" variant="ghost" aria-label={`Excluir ${name}`} onClick={() => setConfirm(true)}>
                <Trash2 />
              </Button>
            </>
          )}
        </>
      )}
    </li>
  );
}

function Layouts() {
  const items = usePresets((s) => s.items);
  const refresh = usePresets((s) => s.refresh);
  useEffect(() => {
    refresh().catch((e) => toast.error("Falha ao carregar layouts", { description: errorMessage(e) }));
  }, [refresh]);
  return (
    <div className="space-y-6 py-4">
      <p className="text-xs leading-relaxed text-muted">
        Layouts guardam a disposição e os tipos dos panes, não o conteúdo. Aplique pelo menu <b className="text-fg">Layouts</b> na barra de sessões de qualquer workspace.
      </p>
      <section>
        <h2 className="mb-2 text-[10.5px] font-medium uppercase tracking-wider text-subtle">Meus layouts</h2>
        {items.length === 0 ? (
          <div className="rounded-lg border border-border bg-bg-2">
            <EmptyState title="Nenhum layout salvo" description="Use “Salvar layout atual…” no menu Layouts de um workspace." />
          </div>
        ) : (
          <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-bg-2">
            {items.map((p) => (
              <PresetRow key={p.id} id={p.id} name={p.name} />
            ))}
          </ul>
        )}
      </section>
      <section>
        <h2 className="mb-2 text-[10.5px] font-medium uppercase tracking-wider text-subtle">Incluídos no FORGE</h2>
        <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-bg-2">
          {BUILTIN_PRESETS.map((p) => (
            <li key={p.id} className="flex items-center gap-2 px-3 py-2">
              <LayoutTemplate className="size-3.5 text-subtle" />
              <span className="text-[13px]">{p.name}</span>
              <span className="ml-auto text-xs text-subtle">{p.description}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

function Processes() {
  const [items, setItems] = useState<TerminalInfo[] | null>(null);
  const workspaces = useWorkspaces((s) => s.items);
  useEffect(() => {
    let alive = true;
    const load = () => api.terminalList(null).then((l) => alive && setItems(l)).catch(() => alive && setItems([]));
    void load();
    const t = setInterval(load, 2000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  const kill = async (id: string) => {
    try {
      await api.terminalKill(id);
      setItems((l) => l?.filter((t) => t.id !== id) ?? null);
    } catch (e) {
      toast.error("Falha ao encerrar", { description: errorMessage(e) });
    }
  };
  return (
    <div className="py-4">
      <p className="mb-4 text-xs leading-relaxed text-muted">
        Terminais em execução neste FORGE, de todos os workspaces. Eles continuam ativos ao trocar de aba ou workspace e são encerrados ao fechar o app. Entrada e saída nunca são gravadas em disco.
      </p>
      {items === null ? null : items.length === 0 ? (
        <div className="rounded-lg border border-border bg-bg-2">
          <EmptyState title="Nenhum processo ativo" description="Abra um pane Terminal num workspace." />
        </div>
      ) : (
        <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-bg-2">
          {items.map((t) => (
            <li key={t.id} className="flex items-center gap-3 px-3 py-2">
              <StatusDot tone={t.exited ? "neutral" : "success"} />
              <div className="min-w-0 flex-1">
                <div className="text-[13px]">
                  {t.shell.split(/[\\/]/).pop()} <span className="text-subtle">· {workspaces.find((w) => w.id === t.workspaceId)?.name ?? "workspace removido"}</span>
                </div>
                <div className="truncate font-mono text-[11px] text-subtle">
                  {prettyPath(t.cwd)} · {t.exited ? `encerrado (código ${t.exitCode ?? "?"})` : `pid ${t.pid ?? "?"}`} · iniciado {relativeTime(t.createdAt)}
                </div>
              </div>
              <Button size="sm" variant={t.exited ? "ghost" : "danger"} onClick={() => void kill(t.id)}>
                {t.exited ? "Descartar" : "Encerrar"}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Brand() {
  return (
    <div className="py-4">
      <p className="mb-4 text-xs leading-relaxed text-muted">
        O FORGE usa exclusivamente os arquivos oficiais da marca. Quando um arquivo obrigatório está ausente, a interface usa apenas um wordmark tipográfico — nunca uma imagem genérica.
      </p>
      <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-bg-2">
        {brandStatus.map((b) => (
          <li key={b.file} className="flex items-center gap-3 px-3 py-2">
            {b.present ? <CheckCircle2 className="size-4 text-success" /> : <CircleDashed className={cn("size-4", b.required ? "text-warning" : "text-subtle")} />}
            <span className="font-mono text-[12px]">{b.file}</span>
            <span className="ml-auto">
              {b.present ? <Badge tone="success">presente</Badge> : b.required ? <Badge tone="warning">ausente</Badge> : <Badge>opcional</Badge>}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Data({ info }: { info: AppInfo | null }) {
  return (
    <>
      <Row title="Banco de dados local" description={info ? prettyPath(info.dbPath) : "…"}>
        <Button size="sm" onClick={() => api.revealDataDir().catch((e) => toast.error("Falha", { description: errorMessage(e) }))}>
          <Monitor /> Mostrar arquivo
        </Button>
      </Row>
      <Row title="Versão do schema" description="Migrations aplicadas automaticamente ao iniciar.">
        <span className="font-mono text-xs text-muted">{info?.schemaVersion ?? "…"}</span>
      </Row>
      <Row title="Privacidade" description="Todos os dados ficam neste computador. O FORGE não envia telemetria.">
        <Badge tone="success">local</Badge>
      </Row>
    </>
  );
}

export function SettingsView({ section }: { section: SettingsSection }) {
  const navigate = useApp((s) => s.navigate);
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => setInfo(null));
  }, []);

  const current = SECTIONS.find((s) => s.id === section) ?? SECTIONS[0]!;

  return (
    <div className="flex h-full">
      <nav className="w-52 shrink-0 border-r border-border p-3">
        <div className="px-2 pt-1 pb-3 text-[10.5px] font-medium uppercase tracking-wider text-subtle">Configurações</div>
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => navigate({ view: "settings", section: s.id })}
            className={cn(
              "flex h-8 w-full items-center gap-2 rounded-md px-2 text-[13px]",
              s.id === current.id ? "bg-elevated text-fg" : "text-muted hover:bg-elevated/60 hover:text-fg",
            )}
          >
            <s.icon className="size-3.5" /> {s.label}
          </button>
        ))}
      </nav>
      <div className="min-w-0 flex-1 overflow-auto">
        <div className="mx-auto max-w-3xl px-8 py-8">
          <h1 className="mb-2 text-xl font-semibold">{current.label}</h1>
          {current.id === "general" && <General />}
          {current.id === "appearance" && <Appearance />}
          {current.id === "voice" && <VoiceSettings />}
          {current.id === "layouts" && <Layouts />}
          {current.id === "processes" && <Processes />}
          {current.id === "environment" && (
            <div className="pt-3">
              <p className="mb-4 text-xs leading-relaxed text-muted">
                Ferramentas detectadas neste computador (binário no PATH + resposta ao comando de versão). O FORGE prioriza a autenticação nativa de cada CLI.
              </p>
              <EnvironmentTable />
            </div>
          )}
          {current.id === "brand" && <Brand />}
          {current.id === "data" && <Data info={info} />}
          {current.id === "about" && <AboutSection info={info} />}
        </div>
      </div>
    </div>
  );
}
