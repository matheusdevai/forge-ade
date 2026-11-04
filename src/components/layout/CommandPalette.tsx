import { Command } from "cmdk";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { AppWindow, Columns2, FolderOpen, Home, LayoutGrid, LayoutTemplate, Maximize2, Minus, Plus, RefreshCw, Rows2, Bot, Library, Plug, Sparkle, Users, GitBranch, Save, Search, Settings, Sparkles, Workflow, SquareStack, SquareTerminal, Target, Wrench, X, FileSearch, Globe, MonitorPlay, Brain, Mic, Clapperboard, Timer, Activity, ScrollText } from "lucide-react";
import { Kbd } from "@/components/ui";
import { minimizePane, restorePane, splitPane, toggleMaximize } from "@/features/panes/layout";
import { BUILTIN_PRESETS } from "@/features/panes/presets";
import { applyLayoutWithUndo } from "@/features/panes/SessionTabs";
import { PANE_REGISTRY } from "@/features/panes/registry";
import { useLayout } from "@/features/panes/store";
import { openWorkspace, pickAndOpenWorkspace } from "@/features/workspace/actions";
import { modKey, prettyPath } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { useEnvironment, useWorkspaces } from "@/stores/data";
import { ensurePane } from "@/features/panes/ensure";
import { launchAgentPane } from "@/features/agents/actions";
import { saveAllWithFeedback } from "@/features/editor/saveAll";
import { useVoice } from "@/features/voice/store";

function Item({ icon: Icon, label, hint, onSelect, value }: { icon: typeof Home; label: string; hint?: string[]; onSelect: () => void; value?: string }) {
  return (
    <Command.Item
      value={value ?? label}
      onSelect={onSelect}
      className="flex h-9 cursor-default items-center gap-2.5 rounded-md px-2.5 text-[13px] text-muted data-[selected=true]:bg-accent/15 data-[selected=true]:text-fg"
    >
      <Icon className="size-4 text-subtle" />
      <span className="truncate">{label}</span>
      {hint && (
        <span className="ml-auto flex gap-0.5">
          {hint.map((h) => (
            <Kbd key={h}>{h}</Kbd>
          ))}
        </span>
      )}
    </Command.Item>
  );
}

const groupCls =
  "[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-[10.5px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:text-subtle";

export function CommandPalette() {
  const { paletteOpen, setPaletteOpen, navigate, openNewMission, route } = useApp();
  const workspaces = useWorkspaces((s) => s.items);
  const scan = useEnvironment((s) => s.scan);
  const layout = useLayout();
  const inWorkspace = route.view === "workspace" && !!layout.doc;
  const focused = layout.doc?.focusedId ?? null;

  const run = (fn: () => void) => {
    setPaletteOpen(false);
    fn();
  };

  return (
    <DialogPrimitive.Root open={paletteOpen} onOpenChange={setPaletteOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <DialogPrimitive.Content className="fixed top-[14vh] left-1/2 z-50 w-[min(620px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden rounded-xl border border-border bg-surface shadow-2xl shadow-black/60">
          <DialogPrimitive.Title className="sr-only">Paleta de comandos</DialogPrimitive.Title>
          <DialogPrimitive.Description className="sr-only">Busque ações, workspaces e configurações</DialogPrimitive.Description>
          <Command loop label="Paleta de comandos">
            <Command.Input
              autoFocus
              placeholder="Digite um comando ou busque um workspace…"
              className="h-12 w-full border-b border-border bg-transparent px-4 text-[14px] text-fg placeholder:text-subtle focus:outline-none"
            />
            <Command.List className="max-h-[56vh] overflow-auto p-1.5">
              <Command.Empty className="py-8 text-center text-xs text-subtle">Nenhum resultado.</Command.Empty>

              <Command.Group heading="Ações" className={groupCls}>
                <Item icon={Plus} label="New mission" value="nova missão new mission" hint={[modKey, "N"]} onSelect={() => run(() => openNewMission(route.view === "workspace" ? route.id : null))} />
                <Item icon={FolderOpen} label="Open workspace" value="abrir workspace open workspace pasta" hint={[modKey, "O"]} onSelect={() => run(() => void pickAndOpenWorkspace())} />
                <Item icon={RefreshCw} label="Verificar ambiente novamente" onSelect={() => run(() => void scan())} />
              </Command.Group>

              {inWorkspace && (
                <Command.Group heading="Agentes" className={groupCls}>
                  <Item icon={Users} label="Agente — conversar com um perfil (headless)" hint={[modKey, "I"]} onSelect={() => run(() => void ensurePane("agent"))} />
                  <Item icon={Sparkles} label="Abrir Claude Code" onSelect={() => run(() => launchAgentPane("claude", "agent"))} />
                  <Item icon={Bot} label="Abrir Codex" onSelect={() => run(() => launchAgentPane("codex", "agent"))} />
                  <Item icon={Workflow} label="Abrir Gemini" onSelect={() => run(() => launchAgentPane("gemini", "agent"))} />
                </Command.Group>
              )}

              {inWorkspace && (
                <Command.Group heading="Editor" className={groupCls}>
                  <Item icon={FileSearch} label="Abrir arquivo…" hint={[modKey, "P"]} onSelect={() => run(() => useApp.getState().setQuickOpen(true))} />
                  <Item icon={Search} label="Buscar no workspace" hint={[modKey, "⇧", "F"]} onSelect={() => run(() => void ensurePane("search"))} />
                  <Item icon={GitBranch} label="Git — alterações, branches e worktrees" hint={[modKey, "⇧", "G"]} onSelect={() => run(() => void ensurePane("git"))} />
                  <Item icon={Save} label="Salvar todos" hint={[modKey, "⌥", "S"]} onSelect={() => run(() => void saveAllWithFeedback())} />
                </Command.Group>
              )}

              {inWorkspace && (
                <Command.Group heading="Navegador" className={groupCls}>
                  <Item icon={Globe} label="Navegador — abrir página, console e capturas" value="navegador browser" hint={[modKey, "⇧", "B"]} onSelect={() => run(() => void ensurePane("browser"))} />
                  <Item icon={MonitorPlay} label="Prévia — rodar o servidor de desenvolvimento" value="prévia preview dev server" onSelect={() => run(() => void ensurePane("preview"))} />
                </Command.Group>
              )}

              {inWorkspace && focused && (
                <Command.Group heading="Pane em foco" className={groupCls}>
                  <Item icon={SquareTerminal} label="Novo terminal à direita" onSelect={() => run(() => layout.apply((d) => splitPane(d, focused, "horizontal", "terminal")))} />
                  <Item icon={SquareTerminal} label="Novo terminal abaixo" hint={[modKey, "J"]} onSelect={() => run(() => layout.apply((d) => splitPane(d, focused, "vertical", "terminal")))} />
                  <Item icon={Columns2} label="Dividir à direita (Files)" onSelect={() => run(() => layout.apply((d) => splitPane(d, focused, "horizontal", "explorer")))} />
                  <Item icon={Rows2} label="Dividir abaixo (Files)" onSelect={() => run(() => layout.apply((d) => splitPane(d, focused, "vertical", "explorer")))} />
                  <Item icon={Maximize2} label="Maximizar / restaurar pane" hint={[modKey, "⇧", "↵"]} onSelect={() => run(() => layout.apply((d) => toggleMaximize(d, focused)))} />
                  <Item icon={Minus} label="Minimizar pane" onSelect={() => run(() => layout.apply((d) => minimizePane(d, focused)))} />
                  <Item icon={X} label="Fechar pane" onSelect={() => run(() => layout.closePaneById(focused))} />
                  <Item icon={LayoutGrid} label="Restaurar layout padrão" onSelect={() => run(layout.reset)} />
                </Command.Group>
              )}

              {inWorkspace && (layout.doc?.minimized.length ?? 0) > 0 && (
                <Command.Group heading="Panes minimizados" className={groupCls}>
                  {layout.doc!.minimized.map(({ pane }) => (
                    <Item
                      key={pane.id}
                      icon={AppWindow}
                      label={`Restaurar ${PANE_REGISTRY[pane.kind].label}`}
                      value={`restaurar ${PANE_REGISTRY[pane.kind].label} ${pane.id}`}
                      onSelect={() => run(() => layout.apply((d) => restorePane(d, pane.id)))}
                    />
                  ))}
                </Command.Group>
              )}

              {inWorkspace && (
                <Command.Group heading="Sessões e layouts" className={groupCls}>
                  <Item icon={Plus} label="Nova sessão" hint={[modKey, "T"]} onSelect={() => run(() => void layout.newSession())} />
                  {layout.sessions
                    .filter((s) => s.id !== layout.activeId)
                    .map((s) => (
                      <Item key={s.id} icon={SquareStack} label={`Ir para sessão “${s.name}”`} value={`sessão ${s.name} ${s.id}`} onSelect={() => run(() => layout.switchSession(s.id))} />
                    ))}
                  {BUILTIN_PRESETS.map((p) => (
                    <Item key={p.id} icon={LayoutTemplate} label={`Aplicar layout: ${p.name}`} onSelect={() => run(() => applyLayoutWithUndo(p.build(), p.name))} />
                  ))}
                </Command.Group>
              )}

              <Command.Group heading="Navegar" className={groupCls}>
                <Item icon={Home} label="Dashboard" hint={[modKey, "1"]} onSelect={() => run(() => navigate({ view: "dashboard" }))} />
                <Item icon={Target} label="Missões" hint={[modKey, "2"]} onSelect={() => run(() => navigate({ view: "missions" }))} />
                <Item icon={Users} label="Agentes" hint={[modKey, "3"]} onSelect={() => run(() => navigate({ view: "agents" }))} />
                <Item icon={Bot} label="Providers" hint={[modKey, "4"]} onSelect={() => run(() => navigate({ view: "providers" }))} />
                <Item icon={Mic} label="Jarvis — comando de voz" value="jarvis voz voice falar" hint={[modKey, "⇧", "J"]} onSelect={() => run(() => void useVoice.getState().start("jarvis"))} />
                <Item icon={Mic} label="Ditar para o agente em foco" value="ditar ditado voz clock voice" hint={[modKey, "⇧", "D"]} onSelect={() => run(() => void useVoice.getState().start("clock"))} />
                <Item icon={Activity} label="Observabilidade — métricas, custos e processos" value="observabilidade métricas custos tokens processos" onSelect={() => run(() => navigate({ view: "observability" }))} />
                <Item icon={ScrollText} label="Histórico — pesquisar execuções, comandos e erros" value="histórico redline logs pesquisar" onSelect={() => run(() => navigate({ view: "observability", tab: "timeline" }))} />
                <Item icon={Activity} label="Auditoria — ações sensíveis" value="auditoria audit segurança" onSelect={() => run(() => navigate({ view: "observability", tab: "audit" }))} />
                <Item icon={Timer} label="Autopilot — rotinas agendadas" value="autopilot rotinas agendamento automação" hint={[modKey, "0"]} onSelect={() => run(() => navigate({ view: "autopilot" }))} />
                <Item icon={Clapperboard} label="Mídia — imagens, voz, transcrição e vídeo" value="mídia media imagem vídeo áudio" hint={[modKey, "9"]} onSelect={() => run(() => navigate({ view: "media" }))} />
                <Item icon={Brain} label="Memória — decisões, convenções e sugestões" value="memória memory" hint={[modKey, "8"]} onSelect={() => run(() => navigate({ view: "memory" }))} />
                <Item icon={Plug} label="MCP — servidores" hint={[modKey, "7"]} onSelect={() => run(() => navigate({ view: "mcp" }))} />
                <Item icon={Sparkle} label="Skills" hint={[modKey, "6"]} onSelect={() => run(() => navigate({ view: "skills" }))} />
                <Item icon={Library} label="Squads & Receitas" hint={[modKey, "5"]} onSelect={() => run(() => navigate({ view: "library" }))} />
                <Item icon={Library} label="Nova missão a partir de receita…" onSelect={() => run(() => navigate({ view: "library", tab: "recipes" }))} />
                <Item icon={Settings} label="Configurações" hint={[modKey, ","]} onSelect={() => run(() => navigate({ view: "settings", section: "general" }))} />
                <Item icon={Wrench} label="Ambiente e providers" onSelect={() => run(() => navigate({ view: "settings", section: "environment" }))} />
              </Command.Group>

              {workspaces.length > 0 && (
                <Command.Group heading="Workspaces" className={groupCls}>
                  {workspaces.map((w) => (
                    <Item
                      key={w.id}
                      icon={FolderOpen}
                      label={`${w.name} — ${prettyPath(w.path)}`}
                      value={`workspace ${w.name} ${w.path} ${w.id}`}
                      onSelect={() => run(() => void openWorkspace(w.id))}
                    />
                  ))}
                </Command.Group>
              )}
            </Command.List>
          </Command>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
