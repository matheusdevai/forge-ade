/**
 * Pane type registry. A kind is `available` only when it is fully implemented.
 * Unavailable kinds are listed (disabled) with the milestone that delivers them,
 * so the roadmap is visible without shipping fake functionality.
 */
import { lazy, type ComponentType } from "react";
import {
  Activity,
  Bot,
  FileCode2,
  FolderTree,
  GitBranch,
  Globe,
  Search,
  LayoutDashboard,
  MonitorPlay,
  ScrollText,
  Sparkles,
  SquareTerminal,
  Workflow,
  type LucideIcon,
} from "lucide-react";
import type { PaneKind, PaneNode } from "./layout";
import { ExplorerPane } from "./kinds/ExplorerPane";
import { OverviewPane } from "./kinds/OverviewPane";

// xterm.js is heavy: load the terminal pane on demand.
const TerminalPane = lazy(() => import("./kinds/TerminalPane").then((m) => ({ default: m.TerminalPane })));
// Monaco is heavy too.
const EditorPane = lazy(() => import("@/features/editor/EditorPane").then((m) => ({ default: m.EditorPane })));
const AgentRunPane = lazy(() => import("@/features/agents/AgentRunPane").then((m) => ({ default: m.AgentRunPane })));
const AgentPane = lazy(() => import("@/features/agents/AgentPane").then((m) => ({ default: m.AgentPane })));
const GitPane = lazy(() => import("@/features/git/GitPane").then((m) => ({ default: m.GitPane })));
const BrowserPane = lazy(() => import("@/features/browser/BrowserPane").then((m) => ({ default: m.BrowserPane })));
const PreviewPane = lazy(() => import("@/features/browser/BrowserPane").then((m) => ({ default: m.PreviewPane })));
const LogsPane = lazy(() => import("@/features/observability/ObservabilityView").then((m) => ({ default: m.LogsPane })));
const MonitorPane = lazy(() => import("@/features/observability/ObservabilityView").then((m) => ({ default: m.MonitorPane })));
const SearchPane = lazy(() => import("@/features/editor/SearchPane").then((m) => ({ default: m.SearchPane })));

export type PaneProps = { pane: PaneNode; workspaceId: string };

export type PaneKindDef = {
  kind: PaneKind;
  label: string;
  icon: LucideIcon;
  group: "workspace" | "agent" | "tools";
  milestone: string;
  component?: ComponentType<PaneProps>;
};

export const PANE_REGISTRY: Record<PaneKind, PaneKindDef> = {
  explorer: { kind: "explorer", label: "Files", icon: FolderTree, group: "workspace", milestone: "M01", component: ExplorerPane },
  overview: { kind: "overview", label: "Overview", icon: LayoutDashboard, group: "workspace", milestone: "M01", component: OverviewPane },
  search: { kind: "search", label: "Busca", icon: Search, group: "workspace", milestone: "M04", component: SearchPane },
  terminal: { kind: "terminal", label: "Terminal", icon: SquareTerminal, group: "tools", milestone: "M03", component: TerminalPane },
  editor: { kind: "editor", label: "Editor", icon: FileCode2, group: "workspace", milestone: "M04", component: EditorPane },
  git: { kind: "git", label: "Git", icon: GitBranch, group: "tools", milestone: "M05", component: GitPane },
  agent: { kind: "agent", label: "Agente", icon: Bot, group: "agent", milestone: "M07", component: AgentRunPane },
  claude: { kind: "claude", label: "Claude Code", icon: Sparkles, group: "agent", milestone: "M06", component: AgentPane },
  codex: { kind: "codex", label: "Codex", icon: Bot, group: "agent", milestone: "M06", component: AgentPane },
  gemini: { kind: "gemini", label: "Gemini", icon: Workflow, group: "agent", milestone: "M06", component: AgentPane },
  browser: { kind: "browser", label: "Browser", icon: Globe, group: "tools", milestone: "M13", component: BrowserPane },
  preview: { kind: "preview", label: "Preview", icon: MonitorPlay, group: "tools", milestone: "M13", component: PreviewPane },
  logs: { kind: "logs", label: "Logs", icon: ScrollText, group: "tools", milestone: "M19", component: LogsPane },
  monitor: { kind: "monitor", label: "Monitoramento", icon: Activity, group: "tools", milestone: "M19", component: MonitorPane },
};

export const PANE_GROUPS: { id: PaneKindDef["group"]; label: string }[] = [
  { id: "workspace", label: "Workspace" },
  { id: "agent", label: "Agentes" },
  { id: "tools", label: "Ferramentas" },
];

export const isAvailable = (kind: PaneKind) => !!PANE_REGISTRY[kind].component;
