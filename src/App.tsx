import { useEffect, useState } from "react";
import { Toaster } from "sonner";
import { TooltipProvider } from "@/components/ui";
import { CommandPalette } from "@/components/layout/CommandPalette";
import { Sidebar } from "@/components/layout/Sidebar";
import { TitleBar } from "@/components/layout/TitleBar";
import { Dashboard } from "@/features/dashboard/Dashboard";
import { MissionsView } from "@/features/missions/MissionsView";
import { MissionView } from "@/features/missions/MissionView";
import { ProvidersView } from "@/features/agents/ProvidersView";
import { AgentsView } from "@/features/agents/AgentsView";
import { LibraryView } from "@/features/library/LibraryView";
import { SkillsView } from "@/features/skills/SkillsView";
import { McpView } from "@/features/mcp/McpView";
import { MemoryView } from "@/features/memory/MemoryView";
import { MediaView } from "@/features/media/MediaView";
import { AutopilotView } from "@/features/autopilot/AutopilotView";
import { ObservabilityView } from "@/features/observability/ObservabilityView";
import { useAutopilotAlerts } from "@/features/autopilot/alerts";
import { useProviders } from "@/stores/providers";
import { NewMissionDialog } from "@/features/missions/NewMissionDialog";
import { VoiceHud } from "@/features/voice/VoiceHud";
import { QuickOpen } from "@/features/editor/QuickOpen";
import { UnsavedChangesGuard } from "@/features/editor/UnsavedChangesGuard";
import { Onboarding } from "@/features/onboarding/Onboarding";
import { SettingsView } from "@/features/settings/SettingsView";
import { openWorkspace } from "@/features/workspace/actions";
import { WorkspaceView } from "@/features/workspace/WorkspaceView";
import { useGlobalHotkeys } from "@/lib/hotkeys";
import { errorMessage, isDesktop } from "@/lib/ipc";
import { useApp } from "@/stores/app";
import { useActivity, useEnvironment, useMissions, useWorkspaces } from "@/stores/data";
import { useSettings } from "@/stores/settings";

async function bootstrap() {
  await useSettings.getState().load();
  await Promise.all([useWorkspaces.getState().refresh(), useMissions.getState().refresh(), useActivity.getState().refresh()]);
  void useEnvironment.getState().scan();
  void useProviders.getState().refresh().catch(() => undefined);

  const s = useSettings.getState().values;
  const last = s["session.lastWorkspaceId"];
  if (s["onboarding.completed"] && s["session.restore"] && last && useWorkspaces.getState().items.some((w) => w.id === last)) {
    await openWorkspace(last);
  }
}

function Shell() {
  const route = useApp((s) => s.route);
  useGlobalHotkeys();
  useAutopilotAlerts();
  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="flex min-w-0 flex-1 flex-col">
        <TitleBar />
        <div className="min-h-0 flex-1">
          {route.view === "dashboard" && <Dashboard />}
          {route.view === "missions" && <MissionsView />}
          {route.view === "mission" && <MissionView key={route.id} id={route.id} />}
          {route.view === "providers" && <ProvidersView />}
          {route.view === "agents" && <AgentsView />}
          {route.view === "library" && <LibraryView tab={route.tab} />}
          {route.view === "skills" && <SkillsView id={route.id} />}
          {route.view === "mcp" && <McpView id={route.id} />}
          {route.view === "memory" && <MemoryView workspaceId={route.workspaceId} />}
          {route.view === "media" && <MediaView />}
          {route.view === "autopilot" && <AutopilotView />}
          {route.view === "observability" && <ObservabilityView tab={route.tab} />}
          {route.view === "workspace" && <WorkspaceView id={route.id} />}
          {route.view === "settings" && <SettingsView section={route.section} />}
        </div>
      </main>
      <CommandPalette />
      <QuickOpen />
      <NewMissionDialog />
      <VoiceHud />
      <UnsavedChangesGuard />
    </div>
  );
}

export default function App() {
  const [error, setError] = useState<string | null>(null);
  const loaded = useSettings((s) => s.loaded);
  const onboarded = useSettings((s) => s.values["onboarding.completed"]);
  const reduceMotion = useSettings((s) => s.values["ui.reduceMotion"]);

  useEffect(() => {
    if (!isDesktop()) return;
    bootstrap().catch((e) => setError(errorMessage(e)));
  }, []);

  useEffect(() => {
    document.documentElement.dataset.reduceMotion = String(reduceMotion);
  }, [reduceMotion]);

  if (!isDesktop()) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-center text-[13px] text-muted">
        O FORGE é um aplicativo desktop. Execute <code className="mx-1 font-mono text-fg">npm run app:dev</code> para abri-lo.
      </div>
    );
  }

  return (
    <TooltipProvider delayDuration={400}>
      {error ? (
        <div className="flex h-full items-center justify-center p-8 text-center text-[13px] text-error">Falha ao iniciar o FORGE: {error}</div>
      ) : !loaded ? (
        <div className="h-full bg-bg" data-tauri-drag-region />
      ) : onboarded ? (
        <Shell />
      ) : (
        <Onboarding />
      )}
      <Toaster
        theme="dark"
        position="bottom-right"
        toastOptions={{ className: "!bg-elevated !border-border !text-fg !text-[13px]" }}
      />
    </TooltipProvider>
  );
}
