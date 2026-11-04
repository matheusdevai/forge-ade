// @vitest-environment jsdom
/**
 * Render smoke tests: every screen and the main workspace panes must mount
 * without crashing or looping (e.g. React error #185 from a zustand selector
 * that returns a new array on every read). The Tauri bridge is stubbed here
 * ONLY for tests: calls stay pending, so components render their loading
 * states with the seeded store data.
 */
import React from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: ((m: unknown) => void) | null = null;
  },
  invoke: () => new Promise(() => {}),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));
vi.mock("@/lib/ipc", async (orig) => {
  const real = await orig<typeof import("@/lib/ipc")>();
  const pending = () => new Promise(() => {});
  return { ...real, isDesktop: () => false, api: new Proxy({}, { get: () => pending }) };
});

class Boundary extends React.Component<{ onError: (e: Error) => void; children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(e: Error) {
    this.props.onError(e);
  }
  render() {
    return this.state.failed ? null : this.props.children;
  }
}

const WS = { id: "w1", name: "proj", path: "/tmp/proj", pinned: false, createdAt: 1, lastOpenedAt: 2 };
const MISSION = {
  id: "m1", workspaceId: "w1", title: "Pagamentos", briefing: "b", mode: "free", status: "draft", createdAt: 1, updatedAt: 2,
  planRunId: null, planning: false, maxParallel: 1, requireReview: true, baseBranch: null, baseCommit: null, approvedAt: null,
  integratedAt: null, maxAttempts: 2, maxRuns: 20, budgetUsd: null, report: null, squadId: null, recipeId: null, harness: false,
};

async function mount(el: React.ReactElement) {
  const errors: Error[] = [];
  const onWindowError = (e: ErrorEvent) => {
    errors.push(e.error ?? new Error(e.message));
    e.preventDefault();
  };
  window.addEventListener("error", onWindowError);
  const spy = vi.spyOn(console, "error").mockImplementation(() => {});
  await act(async () => {
    const { TooltipProvider } = await import("@/components/ui");
    // Same providers as App.tsx.
    render(
      <TooltipProvider>
        <Boundary onError={(e) => errors.push(e)}>{el}</Boundary>
      </TooltipProvider>,
    );
    await new Promise((r) => setTimeout(r, 30));
  });
  spy.mockRestore();
  window.removeEventListener("error", onWindowError);
  return errors;
}

beforeAll(async () => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, onchange: null, dispatchEvent: () => false })) as unknown as typeof window.matchMedia;
  const { useWorkspaces, useMissions } = await import("@/stores/data");
  useWorkspaces.setState({ items: [WS], loaded: true });
  useMissions.setState({ items: [MISSION] as never, loaded: true });
});

afterEach(() => cleanup());

describe("screens render without crashing or looping", () => {
  const screens: [string, () => Promise<React.ReactElement>][] = [
    ["Dashboard", async () => React.createElement((await import("@/features/dashboard/Dashboard")).Dashboard)],
    ["Missões", async () => React.createElement((await import("@/features/missions/MissionsView")).MissionsView)],
    ["Agentes", async () => React.createElement((await import("@/features/agents/AgentsView")).AgentsView)],
    ["Providers", async () => React.createElement((await import("@/features/agents/ProvidersView")).ProvidersView)],
    ["Biblioteca", async () => React.createElement((await import("@/features/library/LibraryView")).LibraryView)],
    ["Skills", async () => React.createElement((await import("@/features/skills/SkillsView")).SkillsView, {})],
    ["MCP", async () => React.createElement((await import("@/features/mcp/McpView")).McpView, {})],
    ["Memória", async () => React.createElement((await import("@/features/memory/MemoryView")).MemoryView, {})],
    ["Mídia", async () => React.createElement((await import("@/features/media/MediaView")).MediaView)],
    ["Autopilot", async () => React.createElement((await import("@/features/autopilot/AutopilotView")).AutopilotView)],
    ["Observabilidade", async () => React.createElement((await import("@/features/observability/ObservabilityView")).ObservabilityView, {})],
    ["Configurações · Geral", async () => React.createElement((await import("@/features/settings/SettingsView")).SettingsView, { section: "general" })],
    ["Configurações · Voz", async () => React.createElement((await import("@/features/settings/SettingsView")).SettingsView, { section: "voice" })],
    ["Configurações · Sobre", async () => React.createElement((await import("@/features/settings/SettingsView")).SettingsView, { section: "about" })],
  ];
  for (const [name, make] of screens) {
    it(name, async () => {
      expect((await mount(await make())).map((e) => e.message)).toEqual([]);
    });
  }
});

describe("workspace panes render without crashing or looping", () => {
  it("Overview pane (regression: React #185)", async () => {
    const { OverviewPane } = await import("@/features/panes/kinds/OverviewPane");
    const pane = { type: "pane" as const, id: "p1", kind: "overview" as const };
    expect((await mount(<OverviewPane pane={pane} workspaceId="w1" />)).map((e) => e.message)).toEqual([]);
  });

  it("Pane grid with a layout and the minimized tray", async () => {
    const { useLayout } = await import("@/features/panes/store");
    useLayout.setState({
      workspaceId: "w1",
      doc: { version: 1, root: { type: "pane", id: "p1", kind: "overview" }, focusedId: "p1", maximizedId: null, minimized: [] },
    } as never);
    const { PaneGrid } = await import("@/features/panes/PaneGrid");
    expect((await mount(<PaneGrid workspaceId="w1" />)).map((e) => e.message)).toEqual([]);
    useLayout.setState({ doc: null } as never);
    expect((await mount(<PaneGrid workspaceId="w1" />)).map((e) => e.message)).toEqual([]);
  });
});
