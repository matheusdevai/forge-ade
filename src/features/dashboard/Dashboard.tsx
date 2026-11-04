import { Activity, BadgeCheck, Bot, Clock, Command, FolderOpen, FolderPlus, Gauge, Pin, Plus, Server, Target } from "lucide-react";
import { Logo, Mascot, hasMascot } from "@/brand";
import { Badge, Button, Card, CardHeader, EmptyState, Kbd, StatusDot } from "@/components/ui";
import { MissionStatusBadge, MODE_INFO } from "@/features/missions/MissionStatusBadge";
import { openWorkspace, pickAndOpenWorkspace } from "@/features/workspace/actions";
import { modKey, prettyPath, relativeTime } from "@/lib/utils";
import { api, type TerminalInfo } from "@/lib/ipc";
import { useEffect, useState } from "react";
import { useApp } from "@/stores/app";
import { useActivity, useMissions, useWorkspaces } from "@/stores/data";
import { useProviders } from "@/stores/providers";

function greeting() {
  const h = new Date().getHours();
  return h < 12 ? "Bom dia" : h < 18 ? "Boa tarde" : "Boa noite";
}

function RunningAgents() {
  const [agents, setAgents] = useState<TerminalInfo[] | null>(null);
  const providers = useProviders((s) => s.items);
  const workspaces = useWorkspaces((s) => s.items);
  useEffect(() => {
    let alive = true;
    const load = () =>
      Promise.all([api.terminalList(null), api.agentRunsList(null, "running", 50)])
        .then(([terms, runs]) => {
          if (!alive) return;
          const interactive = terms.filter((t) => t.providerId && !t.exited);
          const headless: TerminalInfo[] = runs.map((r) => ({
            id: r.id,
            workspaceId: r.workspaceId ?? "",
            shell: "headless",
            cwd: r.cwd,
            pid: null,
            cols: 0,
            rows: 0,
            createdAt: r.startedAt,
            exited: false,
            exitCode: null,
            providerId: r.providerId,
          }));
          setAgents([...interactive, ...headless]);
        })
        .catch(() => alive && setAgents([]));
    void load();
    const t = setInterval(load, 5000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  return (
    <Card>
      <CardHeader title="Agentes em execução" icon={<Bot />} action={agents && agents.length > 0 ? <Badge tone="success">{agents.length}</Badge> : null} />
      {!agents || agents.length === 0 ? (
        <EmptyState title="Nenhum agente em execução" description="Abra um pane de Claude Code, Codex ou Gemini num workspace. Orquestração e streaming estruturado chegam com o Agent Engine (M07)." />
      ) : (
        <ul className="p-2">
          {agents.map((a) => {
            const ws = workspaces.find((w) => w.id === a.workspaceId);
            return (
              <li key={a.id}>
                <button type="button" onClick={() => ws && void openWorkspace(ws.id)} className="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-elevated">
                  <StatusDot tone="success" pulse />
                  <span className="text-[12.5px]">{providers.find((p) => p.id === a.providerId)?.name ?? a.providerId}</span>
                  <span className="truncate text-[11px] text-subtle">
                    {ws?.name ?? "workspace removido"} · {a.shell === "headless" ? "agente" : "interativo"}
                  </span>
                  <span className="ml-auto shrink-0 text-[11px] text-subtle">{relativeTime(a.createdAt)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

export function Dashboard() {
  const workspaces = useWorkspaces((s) => s.items);
  const missions = useMissions((s) => s.items);
  const activity = useActivity((s) => s.items);
  const { openNewMission, navigate, setPaletteOpen } = useApp();

  const providers = useProviders((s) => s.items);
  const providersLoaded = useProviders((s) => s.loaded);
  const activeMissions = missions.filter((m) => !["completed", "cancelled", "failed"].includes(m.status));

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        {/* Hero */}
        <section className="relative overflow-hidden rounded-2xl border border-border bg-[radial-gradient(120%_140%_at_100%_0%,#579bff1f_0%,transparent_45%),linear-gradient(180deg,#141820,#0d1015)] p-7">
          <div className="flex items-center gap-6">
            <div className="min-w-0 flex-1">
              <Logo className="h-7" />
              <h1 className="mt-5 text-2xl font-semibold tracking-tight">{greeting()}. O que vamos construir hoje?</h1>
              <p className="mt-1.5 max-w-xl text-[13px] text-muted">
                Descreva uma missão, abra um workspace e coordene seus agentes a partir daqui.
              </p>
              <div className="mt-6 flex flex-wrap items-center gap-2">
                <Button variant="primary" size="lg" onClick={() => openNewMission()}>
                  <Plus /> NEW MISSION
                </Button>
                <Button size="lg" onClick={() => void pickAndOpenWorkspace()}>
                  <FolderOpen /> OPEN WORKSPACE
                </Button>
                <button type="button" onClick={() => setPaletteOpen(true)} className="ml-2 inline-flex items-center gap-1.5 text-xs text-subtle hover:text-muted">
                  <Command className="size-3" /> Paleta de comandos <Kbd>{modKey}</Kbd>
                  <Kbd>K</Kbd>
                </button>
              </div>
            </div>
            {hasMascot && <Mascot state="online" size={148} className="hidden shrink-0 md:block" />}
          </div>
        </section>

        <div className="mt-6 grid gap-4 lg:grid-cols-3">
          {/* Workspaces */}
          <Card className="lg:col-span-2">
            <CardHeader
              title="Workspaces recentes"
              icon={<FolderOpen />}
              action={
                <Button size="sm" variant="ghost" onClick={() => void pickAndOpenWorkspace()}>
                  <FolderPlus /> Adicionar
                </Button>
              }
            />
            {workspaces.length === 0 ? (
              <EmptyState
                icon={<FolderOpen />}
                title="Nenhum workspace ainda"
                description="Abra uma pasta de projeto local para começar."
                action={
                  <Button size="sm" onClick={() => void pickAndOpenWorkspace()}>
                    <FolderOpen /> Abrir pasta
                  </Button>
                }
              />
            ) : (
              <ul className="p-1.5">
                {workspaces.slice(0, 6).map((w) => (
                  <li key={w.id}>
                    <button type="button" onClick={() => void openWorkspace(w.id)} className="group flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-elevated">
                      <span className="flex size-8 shrink-0 items-center justify-center rounded-md border border-border bg-bg-2 text-[12px] font-semibold text-accent uppercase">
                        {w.name.slice(0, 2)}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1.5 text-[13px] font-medium">
                          {w.name} {w.pinned && <Pin className="size-3 text-subtle" />}
                        </span>
                        <span className="block truncate font-mono text-[11px] text-subtle">{prettyPath(w.path)}</span>
                      </span>
                      <span className="text-[11px] text-subtle">{w.lastOpenedAt ? relativeTime(w.lastOpenedAt) : "nunca aberto"}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Card>

          {/* Providers */}
          <Card>
            <CardHeader
              title="Providers"
              icon={<Server />}
              action={
                <Button size="sm" variant="ghost" onClick={() => navigate({ view: "providers" })}>
                  Detalhes
                </Button>
              }
            />
            <ul className="space-y-0.5 p-2">
              {!providersLoaded && <li className="px-2 py-3 text-xs text-subtle">Detectando providers…</li>}
              {providers.map((p) => (
                <li key={p.id} className="flex items-center gap-2.5 rounded-md px-2 py-1.5">
                  <StatusDot tone={!p.installed ? "neutral" : p.auth.state === "logged_in" ? "success" : "warning"} />
                  <span className={p.installed ? "text-[12.5px]" : "text-[12.5px] text-subtle"}>{p.name}</span>
                  {p.verified && <BadgeCheck className="size-3.5 text-accent" aria-label="verificado" />}
                  <span className="ml-auto text-[11px] text-subtle">
                    {!p.installed ? "não instalado" : p.auth.state === "logged_in" ? (p.auth.plan ?? p.auth.method ?? "logado") : p.auth.state === "logged_out" ? "sem login" : "login ?"}
                  </span>
                </li>
              ))}
            </ul>
            <p className="border-t border-border px-4 py-2.5 text-[11px] leading-relaxed text-subtle">Autenticação nativa de cada CLI. ✓ = testado de verdade nesta versão.</p>
          </Card>

          {/* Missions */}
          <Card className="lg:col-span-2">
            <CardHeader
              title="Missões"
              icon={<Target />}
              action={
                <>
                  <Badge>{activeMissions.length} abertas</Badge>
                  <Button size="sm" variant="ghost" onClick={() => navigate({ view: "missions" })}>
                    Ver todas
                  </Button>
                </>
              }
            />
            {missions.length === 0 ? (
              <EmptyState icon={<Target />} title="Nenhuma missão" description="Use NEW MISSION para registrar um briefing." />
            ) : (
              <ul className="p-1.5">
                {missions.slice(0, 5).map((m) => (
                  <li key={m.id}>
                    <button type="button" onClick={() => navigate({ view: "mission", id: m.id })} className="flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-elevated">
                      <span className="min-w-0 flex-1 truncate text-[13px]">{m.title}</span>
                      <Badge>{MODE_INFO[m.mode].label}</Badge>
                      <MissionStatusBadge status={m.status} />
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Card>

          {/* Agents */}
          <RunningAgents />

          {/* Activity */}
          <Card className="lg:col-span-2">
            <CardHeader title="Atividade recente" icon={<Activity />} />
            {activity.length === 0 ? (
              <EmptyState icon={<Clock />} title="Sem atividade ainda" />
            ) : (
              <ul className="max-h-72 overflow-auto p-2">
                {activity.map((a) => (
                  <li key={a.id} className="flex items-baseline gap-3 rounded-md px-2 py-1.5">
                    <span className="mt-1 size-1.5 shrink-0 translate-y-[-1px] rounded-full bg-accent/70" />
                    <span className="min-w-0 flex-1 text-[12.5px] text-muted">{a.message}</span>
                    <span className="shrink-0 text-[11px] text-subtle">{relativeTime(a.createdAt)}</span>
                  </li>
                ))}
              </ul>
            )}
          </Card>

          {/* Usage */}
          <Card>
            <CardHeader title="Consumo" icon={<Gauge />} />
            <EmptyState
              title="Sem dados de consumo"
              description="O FORGE só exibirá métricas reais informadas pelos providers — nunca estimativas apresentadas como dados. Observabilidade no M19."
            />
          </Card>
        </div>
      </div>
    </div>
  );
}
