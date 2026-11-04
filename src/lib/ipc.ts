/**
 * Typed bridge to the Rust core. Every backend call goes through here so the
 * command surface is documented in one place.
 */
import { Channel, invoke } from "@tauri-apps/api/core";

export type Workspace = {
  id: string;
  name: string;
  path: string;
  pinned: boolean;
  createdAt: number;
  lastOpenedAt: number | null;
};

export type ToolCategory = "core" | "agent" | "local" | "runtime";

export type ToolStatus = {
  id: string;
  name: string;
  category: ToolCategory;
  required: boolean;
  installed: boolean;
  version: string | null;
  path: string | null;
  error: string | null;
  hint: string;
};

export type MissionMode = "free" | "squad" | "agentic";
export type MissionStatus = "draft" | "planned" | "running" | "paused" | "completed" | "failed" | "cancelled";

export type Mission = {
  id: string;
  workspaceId: string | null;
  title: string;
  briefing: string;
  mode: MissionMode;
  status: MissionStatus;
  createdAt: number;
  updatedAt: number;
  planRunId: string | null;
  planning: boolean;
  maxParallel: number;
  requireReview: boolean;
  baseBranch: string | null;
  baseCommit: string | null;
  approvedAt: number | null;
  integratedAt: number | null;
  maxAttempts: number;
  maxRuns: number;
  budgetUsd: number | null;
  report: string | null;
  squadId: string | null;
  recipeId: string | null;
  harness: boolean;
};
export type MaestroSettings = {
  mode: MissionMode;
  maxParallel: number;
  requireReview: boolean;
  maxAttempts: number;
  maxRuns: number;
  budgetUsd: number | null;
};
export type TaskStatus = "pending" | "running" | "review" | "approved" | "rejected" | "failed" | "cancelled" | "skipped";
export type MissionTask = {
  id: string;
  missionId: string;
  position: number;
  key: string;
  title: string;
  description: string;
  acceptance: string;
  profileId: string | null;
  dependsOn: string[];
  status: TaskStatus;
  runId: string | null;
  worktreePath: string | null;
  worktreeBranch: string | null;
  attempts: number;
  feedback: string | null;
  integratedAt: number | null;
  createdAt: number;
  updatedAt: number;
  baseCommit: string | null;
  reviewRunId: string | null;
  reviewVerdict: "approve" | "reject" | null;
  reviewSummary: string | null;
  providerOverride: string | null;
};
export type TaskInput = { key: string; title: string; description: string; acceptance: string; profileId: string | null; dependsOn: string[] };
export type MissionEventItem = { id: number; taskId: string | null; kind: string; message: string; createdAt: number };
export type MissionDetail = {
  mission: Mission;
  tasks: MissionTask[];
  events: MissionEventItem[];
  runs: AgentRun[];
  totals: { runs: number; inputTokens: number; outputTokens: number; costUsd: number | null; costReportedRuns: number; durationMs: number };
};
export type TaskAction = "approve" | "reject" | "retry" | "skip" | "cancel";

export type NewMission = Pick<Mission, "workspaceId" | "title" | "briefing" | "mode">;

export type ActivityEvent = {
  id: number;
  kind: string;
  message: string;
  workspaceId: string | null;
  missionId: string | null;
  createdAt: number;
};

export type DirEntry = { name: string; relPath: string; isDir: boolean; isSymlink: boolean; size: number | null };
export type DirListing = { relPath: string; entries: DirEntry[]; truncated: boolean };
export type FileContent = { relPath: string; size: number; mtimeMs: number; binary: boolean; truncated: boolean; content: string | null };
export type FileMeta = { relPath: string; size: number; mtimeMs: number; isDir: boolean };
export type FileList = { files: string[]; truncated: boolean };
export type SearchOptions = { query: string; caseSensitive: boolean; regex: boolean; wholeWord: boolean; maxMatches?: number };
export type SearchMatch = {
  relPath: string;
  line: number;
  column: number;
  length: number;
  preview: string;
  previewStart: number;
  previewEnd: number;
};
export type SearchResult = { matches: SearchMatch[]; filesWithMatches: number; filesScanned: number; truncated: boolean };
export type GitInfo = { isRepo: boolean; branch: string | null; detachedHead: string | null };

export type Session = {
  id: string;
  workspaceId: string;
  name: string;
  position: number;
  layoutJson: string;
  createdAt: number;
  updatedAt: number;
};
export type SessionsState = { sessions: Session[]; activeId: string | null };
export type Preset = { id: string; name: string; layoutJson: string; createdAt: number };

export type TerminalInfo = {
  id: string;
  workspaceId: string;
  shell: string;
  cwd: string;
  pid: number | null;
  cols: number;
  rows: number;
  createdAt: number;
  exited: boolean;
  exitCode: number | null;
  providerId: string | null;
};

export type ProviderId = "claude" | "codex" | "gemini";
export type AuthState = "logged_in" | "logged_out" | "unknown";
export type ProviderCheck = {
  ok: boolean;
  version: string | null;
  model: string | null;
  message: string;
  durationMs: number;
  inputTokens: number | null;
  outputTokens: number | null;
  costUsd: number | null;
  checkedAt: number;
};
export type ProviderStatus = {
  id: ProviderId;
  name: string;
  vendor: string;
  installed: boolean;
  version: string | null;
  path: string | null;
  auth: { state: AuthState; method: string | null; plan: string | null; detail: string | null };
  configuredModel: string | null;
  modelAliases: string[];
  capabilities: {
    interactive: boolean;
    headless: boolean;
    structuredOutput: boolean;
    resume: boolean;
    mcp: boolean;
    sandbox: boolean;
    approvalModes: string[];
  };
  install: string;
  loginHint: string;
  docs: string;
  lastCheck: ProviderCheck | null;
  verified: boolean;
};
export type TerminalAttach = { info: TerminalInfo; scrollback: string; token: number };
export type TermEvent = { type: "data"; data: string } | { type: "exit"; code: number | null };

export type GitFileStatus = {
  path: string;
  origPath: string | null;
  index: string;
  worktree: string;
  conflicted: boolean;
  untracked: boolean;
};
export type GitStatus = {
  isRepo: boolean;
  prefix: string;
  top: string;
  branch: string | null;
  oid: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: GitFileStatus[];
  truncated: boolean;
  merging: boolean;
  rebasing: boolean;
  hasCommits: boolean;
};
export type GitBlob = { exists: boolean; binary: boolean; tooLarge: boolean; content: string | null };
export type GitCommit = { sha: string; short: string; author: string; email: string; time: number; refs: string[]; subject: string };
export type GitCommitFile = { status: string; path: string; origPath: string | null };
export type GitBranch = {
  name: string;
  remote: boolean;
  current: boolean;
  shortOid: string;
  upstream: string | null;
  track: string | null;
  time: number;
  subject: string;
};
export type GitStash = { index: number; time: number; message: string };
export type GitWorktree = { path: string; head: string | null; branch: string | null; main: boolean; detached: boolean; locked: boolean; prunable: boolean };

export type Permission = "read" | "edit" | "full";
export type AgentProfile = {
  id: string;
  name: string;
  role: string;
  description: string;
  providerId: ProviderId;
  model: string | null;
  instructions: string;
  permission: Permission;
  maxTurns: number | null;
  builtin: boolean;
  createdAt: number;
  updatedAt: number;
  origin: "builtin" | "local" | "import";
};
export type ProfileInput = Omit<AgentProfile, "id" | "builtin" | "createdAt" | "updatedAt" | "origin">;
export type Origin = "builtin" | "local" | "import";
export type SquadMember = { profileId: string; function: string; stage: number };
export type Squad = {
  id: string;
  name: string;
  description: string;
  members: SquadMember[];
  rules: string;
  flow: string;
  defaultMode: MissionMode;
  builtin: boolean;
  origin: Origin;
  createdAt: number;
  updatedAt: number;
};
export type SquadInput = Pick<Squad, "name" | "description" | "members" | "rules" | "flow" | "defaultMode">;
export type RecipeField = { key: string; label: string; placeholder: string; required: boolean };
export type Recipe = {
  id: string;
  name: string;
  objective: string;
  squadId: string | null;
  mode: MissionMode;
  maxParallel: number;
  maxAttempts: number;
  maxRuns: number;
  budgetUsd: number | null;
  requireReview: boolean;
  briefingTemplate: string;
  fields: RecipeField[];
  plan: TaskInput[] | null;
  builtin: boolean;
  origin: Origin;
  createdAt: number;
  updatedAt: number;
};
export type RecipeInput = Omit<Recipe, "id" | "builtin" | "origin" | "createdAt" | "updatedAt">;
export type BundlePreview = {
  kind: "squad" | "recipe";
  name: string;
  profiles: { name: string; providerId: string; permission: Permission; instructions: string }[];
  squad: string | null;
  recipe: string | null;
  warnings: string[];
  bundle: unknown;
};
export type ImportResult = { profiles: number; squadId: string | null; recipeId: string | null };
export type RunStatus = "running" | "completed" | "failed" | "cancelled";
export type AgentRun = {
  id: string;
  workspaceId: string | null;
  profileId: string | null;
  parentRunId: string | null;
  providerId: ProviderId;
  model: string | null;
  permission: Permission;
  prompt: string;
  status: RunStatus;
  sessionId: string | null;
  cwd: string;
  worktreeBranch: string | null;
  baseCommit: string | null;
  resultText: string | null;
  error: string | null;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  costUsd: number | null;
  durationMs: number | null;
  startedAt: number;
  endedAt: number | null;
  skills: string[];
  mcp: string[];
  memory: string[];
};
export type McpTool = { name: string; description: string; inputSchema: unknown };
export type McpServer = {
  id: string;
  name: string;
  label: string;
  transport: "stdio" | "http";
  command: string | null;
  args: string[];
  env: Record<string, string>;
  url: string | null;
  headers: Record<string, string>;
  secretKeys: string[];
  readOnly: boolean;
  enabled: boolean;
  templateId: string | null;
  tools: McpTool[];
  serverInfo: { serverInfo?: { name?: string; version?: string }; protocolVersion?: string } | null;
  lastCheckAt: number | null;
  lastOk: boolean | null;
  lastError: string | null;
  lastLog: string | null;
  createdAt: number;
  updatedAt: number;
};
export type McpServerInput = Pick<McpServer, "label" | "transport" | "command" | "args" | "env" | "url" | "headers" | "secretKeys" | "readOnly" | "enabled">;
export type McpTemplate = {
  id: string;
  label: string;
  description: string;
  transport: "stdio" | "http";
  command: string | null;
  args: string[];
  url: string | null;
  headers: [string, string][];
  inputs: { key: string; label: string; secret: boolean; placeholder: string }[];
  readOnly: boolean;
  requirements: string;
  docs: string;
};
export type McpDetail = { server: McpServer; secretStatus: Record<string, boolean>; links: [string, string][] };
export type McpProbe = {
  ok: boolean;
  protocolVersion: string | null;
  serverInfo: unknown;
  tools: McpTool[];
  error: string | null;
  log: string;
  durationMs: number;
};
export type Skill = {
  id: string;
  slug: string;
  name: string;
  description: string;
  version: number;
  body: string;
  enabled: boolean;
  builtin: boolean;
  origin: "builtin" | "local" | "import";
  source: string | null;
  createdAt: number;
  updatedAt: number;
};
export type SkillVersion = { version: number; name: string; description: string; body: string; note: string; createdAt: number };
export type SkillFile = { path: string; size: number; isScript: boolean; content?: string | null };
export type SkillLinkKind = "profile" | "squad" | "mission";
export type SkillDetail = { skill: Skill; versions: SkillVersion[]; files: SkillFile[]; links: { targetKind: SkillLinkKind; targetId: string }[] };
export type SkillDoc = { name: string; description: string; body: string };
export type SkillPreview = { token: string; doc: SkillDoc; files: SkillFile[]; warnings: string[]; source: string; exists: boolean };
export type AgentEvent =
  | { kind: "session"; session_id: string | null; model: string | null }
  | { kind: "text_delta"; text: string }
  | { kind: "text"; text: string }
  | { kind: "tool_use"; id: string | null; name: string; input: string }
  | { kind: "tool_result"; id: string | null; ok: boolean; output: string }
  | { kind: "warning"; message: string }
  | { kind: "error"; message: string }
  | {
      kind: "result";
      ok: boolean;
      text: string | null;
      error: string | null;
      input_tokens: number | null;
      output_tokens: number | null;
      cache_read_tokens: number | null;
      cost_usd: number | null;
      duration_ms: number | null;
      turns: number | null;
    };
export type StoredAgentEvent = { seq: number; event: AgentEvent; createdAt: number };
export type RunMessage = { type: "event"; seq: number; event: AgentEvent } | { type: "status"; run: AgentRun };
export type RunAttach = { run: AgentRun; events: StoredAgentEvent[]; token: number | null; live: boolean };
export type RunRequest = {
  workspaceId: string;
  profileId: string | null;
  parentRunId: string | null;
  providerId: ProviderId;
  model: string | null;
  permission: Permission;
  instructions: string | null;
  maxTurns: number | null;
  prompt: string;
  useWorktree: boolean;
  confirmFull: boolean;
  extraSkillIds?: string[];
  extraMcpIds?: string[];
};
export type RunChanges = { cwd: string; prefix: string; worktreeBranch: string | null; isRepo: boolean; files: GitFileStatus[]; commits: GitCommit[] };

export type AppInfo = {
  version: string;
  platform: string;
  arch: string;
  dataDir: string;
  dbPath: string;
  schemaVersion: number;
};

export type Settings = {
  "onboarding.completed": boolean;
  "session.restore": boolean;
  "session.lastWorkspaceId": string | null;
  "ui.mascot": boolean;
  "ui.reduceMotion": boolean;
  "workspace.defaultDir": string | null;
  "voice.locale": string | null;
  "voice.onDevice": boolean;
  "voice.speak": boolean;
  "voice.autoSend": boolean;
};
export type SettingKey = keyof Settings;

export class IpcError extends Error {
  constructor(
    public kind: string,
    message: string,
  ) {
    super(message);
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (e && typeof e === "object" && "message" in e) {
      const err = e as { kind?: string; message: string };
      throw new IpcError(err.kind ?? "unknown", err.message);
    }
    throw new IpcError("unknown", String(e));
  }
}

export const isDesktop = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  revealDataDir: () => call<void>("reveal_data_dir"),
  detectEnvironment: () => call<ToolStatus[]>("detect_environment"),

  settingsGetAll: () => call<Settings>("settings_get_all"),
  settingsSet: <K extends SettingKey>(key: K, value: Settings[K]) => call<void>("settings_set", { key, value }),

  workspacesList: () => call<Workspace[]>("workspaces_list"),
  workspacesAdd: (path: string) => call<Workspace>("workspaces_add", { path }),
  workspacesOpen: (id: string) => call<Workspace>("workspaces_open", { id }),
  workspacesSetPinned: (id: string, pinned: boolean) => call<Workspace>("workspaces_set_pinned", { id, pinned }),
  workspacesRename: (id: string, name: string) => call<Workspace>("workspaces_rename", { id, name }),
  workspacesRemove: (id: string) => call<void>("workspaces_remove", { id }),
  workspacesReveal: (id: string) => call<void>("workspaces_reveal", { id }),
  gitInfo: (workspaceId: string) => call<GitInfo>("workspace_git_info", { workspaceId }),

  fsListDir: (workspaceId: string, relPath: string) => call<DirListing>("fs_list_dir", { workspaceId, relPath }),
  fsReadText: (workspaceId: string, relPath: string, maxBytes?: number) =>
    call<FileContent>("fs_read_text", { workspaceId, relPath, maxBytes: maxBytes ?? null }),
  fsWriteText: (workspaceId: string, relPath: string, content: string, expectedMtimeMs: number | null) =>
    call<FileMeta>("fs_write_text", { workspaceId, relPath, content, expectedMtimeMs }),
  fsStat: (workspaceId: string, relPath: string) => call<FileMeta>("fs_stat", { workspaceId, relPath }),
  fsCreate: (workspaceId: string, relPath: string, isDir: boolean) => call<FileMeta>("fs_create", { workspaceId, relPath, isDir }),
  fsRename: (workspaceId: string, from: string, to: string) => call<FileMeta>("fs_rename", { workspaceId, from, to }),
  fsTrash: (workspaceId: string, relPath: string) => call<void>("fs_trash", { workspaceId, relPath }),
  fsListFiles: (workspaceId: string) => call<FileList>("fs_list_files", { workspaceId }),
  fsSearch: (workspaceId: string, options: SearchOptions) => call<SearchResult>("fs_search", { workspaceId, options }),

  sessionsList: (workspaceId: string) => call<SessionsState>("sessions_list", { workspaceId }),
  sessionsCreate: (workspaceId: string, name: string, layoutJson: string) =>
    call<Session>("sessions_create", { workspaceId, name, layoutJson }),
  sessionsRename: (id: string, name: string) => call<Session>("sessions_rename", { id, name }),
  sessionsSaveLayout: (id: string, layoutJson: string) => call<void>("sessions_save_layout", { id, layoutJson }),
  sessionsSetActive: (workspaceId: string, id: string) => call<void>("sessions_set_active", { workspaceId, id }),
  sessionsDelete: (id: string) => call<SessionsState>("sessions_delete", { id }),
  sessionsReorder: (workspaceId: string, ids: string[]) => call<SessionsState>("sessions_reorder", { workspaceId, ids }),

  presetsList: () => call<Preset[]>("presets_list"),
  presetsCreate: (name: string, layoutJson: string) => call<Preset>("presets_create", { name, layoutJson }),
  presetsRename: (id: string, name: string) => call<Preset>("presets_rename", { id, name }),
  presetsDelete: (id: string) => call<void>("presets_delete", { id }),

  terminalSpawn: (
    workspaceId: string,
    relCwd: string | null,
    cols: number,
    rows: number,
    launch?: { providerId: string; mode: "agent" | "login"; model: string | null } | null,
  ) =>
    call<TerminalInfo>("terminal_spawn", {
      workspaceId,
      relCwd,
      cols,
      rows,
      providerId: launch?.providerId ?? null,
      mode: launch?.mode ?? null,
      model: launch?.model ?? null,
    }),

  providersList: () => call<ProviderStatus[]>("providers_list"),
  providerStatus: (id: string) => call<ProviderStatus>("provider_status", { id }),
  providerVerify: (id: string) => call<ProviderCheck>("provider_verify", { id }),
  terminalAttach: (id: string, onEvent: Channel<TermEvent>) => call<TerminalAttach>("terminal_attach", { id, onEvent }),
  terminalDetach: (id: string, token: number) => call<void>("terminal_detach", { id, token }),
  terminalWrite: (id: string, data: string) => call<void>("terminal_write", { id, data }),
  terminalResize: (id: string, cols: number, rows: number) => call<void>("terminal_resize", { id, cols, rows }),
  terminalKill: (id: string) => call<void>("terminal_kill", { id }),
  terminalList: (workspaceId?: string | null) => call<TerminalInfo[]>("terminal_list", { workspaceId: workspaceId ?? null }),

  gitStatus: (ws: string) => call<GitStatus>("git_status", { workspaceId: ws }),
  gitInit: (ws: string) => call<void>("git_init", { workspaceId: ws }),
  gitFileContents: (ws: string, rev: string, path: string) => call<GitBlob>("git_file_contents", { workspaceId: ws, rev, path }),
  gitStage: (ws: string, paths: string[]) => call<void>("git_stage", { workspaceId: ws, paths }),
  gitUnstage: (ws: string, paths: string[]) => call<void>("git_unstage", { workspaceId: ws, paths }),
  gitDiscard: (ws: string, paths: string[]) => call<void>("git_discard", { workspaceId: ws, paths }),
  gitStageContent: (ws: string, path: string, content: string) => call<void>("git_stage_content", { workspaceId: ws, path, content }),
  gitCommit: (ws: string, message: string, amend: boolean) => call<string>("git_commit", { workspaceId: ws, message, amend }),
  gitLog: (ws: string, limit?: number) => call<GitCommit[]>("git_log", { workspaceId: ws, limit: limit ?? null, rev: null }),
  gitCommitFiles: (ws: string, sha: string) => call<GitCommitFile[]>("git_commit_files", { workspaceId: ws, sha }),
  gitBranches: (ws: string) => call<GitBranch[]>("git_branches", { workspaceId: ws }),
  gitSwitch: (ws: string, branch: string, create: boolean, start?: string | null) =>
    call<void>("git_switch", { workspaceId: ws, branch, create, start: start ?? null }),
  gitDeleteBranch: (ws: string, name: string, force: boolean) => call<void>("git_delete_branch", { workspaceId: ws, name, force }),
  gitMerge: (ws: string, branch: string) => call<string>("git_merge", { workspaceId: ws, branch }),
  gitMergeAbort: (ws: string) => call<void>("git_merge_abort", { workspaceId: ws }),
  gitFetch: (ws: string) => call<string>("git_fetch", { workspaceId: ws }),
  gitPull: (ws: string) => call<string>("git_pull", { workspaceId: ws }),
  gitPush: (ws: string) => call<string>("git_push", { workspaceId: ws }),
  gitStashList: (ws: string) => call<GitStash[]>("git_stash_list", { workspaceId: ws }),
  gitStashPush: (ws: string, message: string | null) => call<void>("git_stash_push", { workspaceId: ws, message }),
  gitStashAction: (ws: string, action: "apply" | "pop" | "drop", index: number) => call<void>("git_stash_action", { workspaceId: ws, action, index }),
  gitWorktrees: (ws: string) => call<GitWorktree[]>("git_worktrees", { workspaceId: ws }),
  gitWorktreeAdd: (ws: string, branch: string, newBranch: boolean, base: string | null, register: boolean) =>
    call<{ worktree: GitWorktree; workspace: Workspace | null }>("git_worktree_add", { workspaceId: ws, branch, newBranch, base, register }),
  gitWorktreeRemove: (ws: string, path: string, force: boolean) => call<void>("git_worktree_remove", { workspaceId: ws, path, force }),

  agentProfilesList: () => call<AgentProfile[]>("agent_profiles_list"),
  agentProfileCreate: (input: ProfileInput) => call<AgentProfile>("agent_profile_create", { input }),
  agentProfileUpdate: (id: string, input: ProfileInput) => call<AgentProfile>("agent_profile_update", { id, input }),
  agentProfileDelete: (id: string) => call<void>("agent_profile_delete", { id }),
  agentProfileRestore: (id: string) => call<AgentProfile>("agent_profile_restore", { id }),
  agentRunStart: (request: RunRequest) => call<AgentRun>("agent_run_start", { request }),
  agentRunAttach: (id: string, onEvent: Channel<RunMessage>) => call<RunAttach>("agent_run_attach", { id, onEvent }),
  agentRunDetach: (id: string, token: number) => call<void>("agent_run_detach", { id, token }),
  agentRunCancel: (id: string) => call<void>("agent_run_cancel", { id }),
  agentRunsList: (workspaceId?: string | null, status?: RunStatus | null, limit?: number) =>
    call<AgentRun[]>("agent_runs_list", { workspaceId: workspaceId ?? null, status: status ?? null, limit: limit ?? null }),
  agentRunDelete: (id: string) => call<void>("agent_run_delete", { id }),
  agentRunChanges: (id: string) => call<RunChanges>("agent_run_changes", { id }),

  missionDetail: (id: string) => call<MissionDetail>("mission_detail", { id }),
  missionUpdate: (id: string, title: string, briefing: string, workspaceId: string | null) =>
    call<Mission>("mission_update", { id, title, briefing, workspaceId }),
  missionMaestro: (id: string, settings: MaestroSettings) => call<Mission>("mission_maestro", { id, settings }),
  missionPlan: (id: string) => call<AgentRun>("mission_plan", { id }),
  missionSaveTasks: (id: string, tasks: TaskInput[]) => call<MissionTask[]>("mission_save_tasks", { id, tasks }),
  missionStart: (id: string) => call<void>("mission_start", { id }),
  missionPause: (id: string) => call<void>("mission_pause", { id }),
  missionResume: (id: string) => call<void>("mission_resume", { id }),
  missionCancelRun: (id: string) => call<void>("mission_cancel_run", { id }),
  missionIntegrate: (id: string) => call<number>("mission_integrate", { id }),
  missionCleanup: (id: string) => call<number>("mission_cleanup", { id }),
  missionTaskAction: (missionId: string, taskId: string, action: TaskAction, feedback?: string | null) =>
    call<void>("mission_task_action", { missionId, taskId, action, feedback: feedback ?? null }),

  squadsList: () => call<Squad[]>("squads_list"),
  squadCreate: (input: SquadInput) => call<Squad>("squad_create", { input }),
  squadUpdate: (id: string, input: SquadInput) => call<Squad>("squad_update", { id, input }),
  squadDelete: (id: string) => call<void>("squad_delete", { id }),
  squadRestore: (id: string) => call<Squad>("squad_restore", { id }),
  recipesList: () => call<Recipe[]>("recipes_list"),
  recipeCreate: (input: RecipeInput) => call<Recipe>("recipe_create", { input }),
  recipeUpdate: (id: string, input: RecipeInput) => call<Recipe>("recipe_update", { id, input }),
  recipeDelete: (id: string) => call<void>("recipe_delete", { id }),
  recipeRestore: (id: string) => call<Recipe>("recipe_restore", { id }),
  missionFromRecipe: (recipeId: string, workspaceId: string | null, title: string, values: Record<string, string>) =>
    call<Mission>("mission_from_recipe", { recipeId, workspaceId, title, values }),
  recipeFromMission: (missionId: string, name: string) => call<Recipe>("recipe_from_mission", { missionId, name }),
  missionSetSquad: (id: string, squadId: string | null) => call<Mission>("mission_set_squad", { id, squadId }),
  bundleExport: (kind: "squad" | "recipe", id: string) => call<string | null>("bundle_export", { kind, id }),
  bundleOpen: () => call<BundlePreview | null>("bundle_open"),
  bundleImport: (bundleData: unknown) => call<ImportResult>("bundle_import", { bundleData }),

  skillsList: () => call<Skill[]>("skills_list"),
  skillDetail: (id: string) => call<SkillDetail>("skill_detail", { id }),
  skillCreate: (doc: SkillDoc) => call<Skill>("skill_create", { doc }),
  skillUpdate: (id: string, doc: SkillDoc, note?: string) => call<Skill>("skill_update", { id, doc, note: note ?? null }),
  skillRestoreVersion: (id: string, version: number) => call<Skill>("skill_restore_version", { id, version }),
  skillRestoreBuiltin: (id: string) => call<Skill>("skill_restore_builtin", { id }),
  skillSetEnabled: (id: string, enabled: boolean) => call<Skill>("skill_set_enabled", { id, enabled }),
  skillDelete: (id: string) => call<void>("skill_delete", { id }),
  skillSetLink: (skillId: string, targetKind: SkillLinkKind, targetId: string, linked: boolean) =>
    call<void>("skill_set_link", { skillId, targetKind, targetId, linked }),
  skillsLinked: (targetKind: SkillLinkKind, targetId: string) => call<Skill[]>("skills_linked", { targetKind, targetId }),
  skillPick: (folder: boolean) => call<SkillPreview | null>("skill_pick", { folder }),
  skillInstall: (token: string, updateExisting: boolean) => call<Skill>("skill_install", { token, updateExisting }),
  skillExport: (id: string) => call<string | null>("skill_export", { id }),

  mcpList: () => call<McpServer[]>("mcp_list"),
  mcpDetail: (id: string) => call<McpDetail>("mcp_detail", { id }),
  mcpTemplates: () => call<McpTemplate[]>("mcp_templates"),
  mcpCreate: (input: McpServerInput) => call<McpServer>("mcp_create", { input }),
  mcpCreateFromTemplate: (templateId: string, label: string | null, values: Record<string, string>) =>
    call<McpServer>("mcp_create_from_template", { templateId, label, values }),
  mcpUpdate: (id: string, input: McpServerInput) => call<McpServer>("mcp_update", { id, input }),
  mcpDelete: (id: string) => call<void>("mcp_delete", { id }),
  mcpSetEnabled: (id: string, enabled: boolean) => call<McpServer>("mcp_set_enabled", { id, enabled }),
  mcpSetSecret: (id: string, key: string, value: string) => call<void>("mcp_set_secret", { id, key, value }),
  mcpClearSecret: (id: string, key: string) => call<void>("mcp_clear_secret", { id, key }),
  mcpTest: (id: string) => call<McpProbe>("mcp_test", { id }),
  mcpSetLink: (serverId: string, targetKind: SkillLinkKind, targetId: string, linked: boolean) =>
    call<void>("mcp_set_link", { serverId, targetKind, targetId, linked }),
  mcpLinked: (targetKind: SkillLinkKind, targetId: string) => call<McpServer[]>("mcp_linked", { targetKind, targetId }),

  missionsList: (workspaceId?: string | null, limit?: number) =>
    call<Mission[]>("missions_list", { workspaceId: workspaceId ?? null, limit: limit ?? null }),
  missionsCreate: (input: NewMission) => call<Mission>("missions_create", { input }),
  missionsCancel: (id: string) => call<Mission>("missions_cancel", { id }),
  missionsDelete: (id: string) => call<void>("missions_delete", { id }),

  activityRecent: (limit?: number) => call<ActivityEvent[]>("activity_recent", { limit: limit ?? null }),

  memoryList: (filter: MemoryFilter) => call<Memory[]>("memory_list", { filter }),
  memoryCounts: (workspaceId?: string | null) => call<MemoryCounts>("memory_counts", { workspaceId: workspaceId ?? null }),
  memoryCreate: (input: MemoryInput) => call<Memory>("memory_create", { input }),
  memoryUpdate: (id: string, input: MemoryInput) => call<Memory>("memory_update", { id, input }),
  memorySetStatus: (id: string, status: "active" | "archived") => call<Memory>("memory_set_status", { id, status }),
  memorySetPinned: (id: string, pinned: boolean) => call<Memory>("memory_set_pinned", { id, pinned }),
  memoryApprove: (id: string) => call<Memory>("memory_approve", { id }),
  memoryDelete: (id: string) => call<void>("memory_delete", { id }),
  memoryPreview: (workspaceId: string | null, missionId: string | null, profileId: string | null) =>
    call<MemoryPreview>("memory_preview", { workspaceId, missionId, profileId }),
  voiceStatus: () => call<VoiceStatus>("voice_status"),
  voiceAuthorize: () => call<VoiceStatus>("voice_authorize"),
  voiceLocales: () => call<VoiceLocale[]>("voice_locales"),
  voiceListen: (locale: string, onDevice: boolean, onEvent: Channel<VoiceEvent>, silence?: number) =>
    call<number>("voice_listen", { locale, onDevice, silence: silence ?? null, onEvent }),
  voiceStop: () => call<void>("voice_stop"),
  voiceCancel: () => call<void>("voice_cancel"),
  voiceSpeak: (text: string) => call<void>("voice_speak", { text }),
  voiceStopSpeaking: () => call<void>("voice_stop_speaking"),
  agentsPauseAll: (dryRun: boolean) => call<PausePlan>("agents_pause_all", { dryRun }),
  voiceSelfTest: (locale: string, onDevice: boolean) => call<VoiceSelfTest>("voice_self_test", { locale, onDevice }),
  harnessRecommend: (text: string, preference: HarnessPreference) => call<Recommendation>("harness_recommend", { text, preference }),
  harnessUsage: () => call<UsageStat[]>("harness_usage"),
  missionSetHarness: (id: string, enabled: boolean) => call<Mission>("mission_set_harness", { id, enabled }),
  mediaStatus: () => call<MediaStatus>("media_status"),
  mediaSetKey: (key: string) => call<GeminiStatus>("media_set_key", { key }),
  mediaClearKey: () => call<GeminiStatus>("media_clear_key"),
  mediaTest: () => call<GeminiStatus>("media_test"),
  mediaSetModel: (op: MediaOp, model: string | null) => call<GeminiStatus>("media_set_model", { op, model }),
  mediaPick: (kind: "image" | "audio") => call<[string, string] | null>("media_pick", { kind }),
  mediaGenerateImage: (workspaceId: string | null, prompt: string) => call<MediaAsset>("media_generate_image", { workspaceId, prompt }),
  mediaAnalyzeImage: (workspaceId: string | null, input: MediaInput, prompt: string, provider: "gemini" | "claude") =>
    call<MediaAsset>("media_analyze_image", { workspaceId, input, prompt, provider }),
  mediaTts: (workspaceId: string | null, text: string, voice: string) => call<MediaAsset>("media_tts", { workspaceId, text, voice }),
  mediaTranscribe: (workspaceId: string | null, input: MediaInput, provider: "gemini" | "macos", locale: string) =>
    call<MediaAsset>("media_transcribe", { workspaceId, input, provider, locale }),
  mediaVideoStart: (workspaceId: string | null, prompt: string) => call<MediaAsset>("media_video_start", { workspaceId, prompt }),
  mediaList: (workspaceId?: string | null, limit?: number) => call<MediaAsset[]>("media_list", { workspaceId: workspaceId ?? null, limit: limit ?? null }),
  mediaRead: (id: string) => call<{ mime: string; data: string }>("media_read", { id }),
  mediaDelete: (id: string) => call<void>("media_delete", { id }),
  mediaReveal: (id: string) => call<void>("media_reveal", { id }),
  mediaSaveToWorkspace: (id: string, workspaceId: string, relDir: string) => call<string>("media_save_to_workspace", { id, workspaceId, relDir }),
  autopilotList: (workspaceId?: string | null) => call<Routine[]>("autopilot_list", { workspaceId: workspaceId ?? null }),
  autopilotCreate: (input: RoutineInput) => call<Routine>("autopilot_create", { input }),
  autopilotUpdate: (id: string, input: RoutineInput) => call<Routine>("autopilot_update", { id, input }),
  autopilotSetEnabled: (id: string, enabled: boolean) => call<Routine>("autopilot_set_enabled", { id, enabled }),
  autopilotDelete: (id: string) => call<void>("autopilot_delete", { id }),
  autopilotRunNow: (id: string) => call<RoutineRun>("autopilot_run_now", { id }),
  autopilotCancel: (runId: string) => call<void>("autopilot_cancel", { runId }),
  autopilotRuns: (routineId?: string | null, limit?: number) => call<RoutineRun[]>("autopilot_runs", { routineId: routineId ?? null, limit: limit ?? null }),
  obsMetrics: (range: "24h" | "7d" | "30d" | "90d", workspaceId?: string | null) => call<Metrics>("obs_metrics", { range, workspaceId: workspaceId ?? null }),
  obsTimeline: (query: TimelineQuery) => call<TimelineItem[]>("obs_timeline", { query }),
  obsReplay: (runId: string) => call<ReplayStep[]>("obs_replay", { runId }),
  obsProcesses: () => call<ProcessInfo[]>("obs_processes"),
  appCheckUpdate: () => call<UpdateInfo>("app_check_update"),
  appInstallUpdate: () => call<void>("app_install_update"),
  crashList: () => call<CrashReport[]>("crash_list"),
  crashRead: (file: string) => call<string>("crash_read", { file }),
  crashClear: () => call<number>("crash_clear"),
  crashReportUi: (summary: string, details: string) => call<void>("crash_report_ui", { summary, details }),
  crashReveal: () => call<void>("crash_reveal"),
  browserStatus: () => call<BrowserStatus>("browser_status"),
  browserOpen: (workspaceId: string, url: string | null, width: number, height: number, mobile = false) =>
    call<BrowserTab>("browser_open", { workspaceId, url, width, height, mobile }),
  browserList: (workspaceId?: string | null) => call<BrowserTab[]>("browser_list", { workspaceId: workspaceId ?? null }),
  browserAttach: (id: string, onEvent: Channel<BrowserEvent>) => call<BrowserAttach>("browser_attach", { id, onEvent }),
  browserDetach: (id: string, token: number) => call<void>("browser_detach", { id, token }),
  browserNavigate: (id: string, url: string) => call<string>("browser_navigate", { id, url }),
  browserGo: (id: string, delta: number) => call<void>("browser_go", { id, delta }),
  browserReload: (id: string, ignoreCache = false) => call<void>("browser_reload", { id, ignoreCache }),
  browserNavState: (id: string) => call<NavState>("browser_nav_state", { id }),
  browserViewport: (id: string, width: number, height: number, mobile: boolean) => call<BrowserTab>("browser_viewport", { id, width, height, mobile }),
  browserMouse: (id: string, input: MouseInput) => call<void>("browser_mouse", { id, input }),
  browserKey: (id: string, input: KeyInput) => call<void>("browser_key", { id, input }),
  browserInsertText: (id: string, text: string) => call<void>("browser_insert_text", { id, text }),
  browserConsole: (id: string) => call<ConsoleEntry[]>("browser_console", { id }),
  browserConsoleClear: (id: string) => call<void>("browser_console_clear", { id }),
  browserScreenshot: (id: string, fullPage = false) => call<Shot>("browser_screenshot", { id, fullPage }),
  browserVerify: (id: string) => call<VerifyReport>("browser_verify", { id }),
  browserClose: (id: string) => call<void>("browser_close", { id }),
  browserRevealShot: (path: string) => call<void>("browser_reveal_shot", { path }),
  previewDetect: (workspaceId: string) => call<PreviewInfo>("preview_detect", { workspaceId }),
  previewStart: (workspaceId: string, script: string, cols: number, rows: number) =>
    call<TerminalInfo>("preview_start", { workspaceId, script, cols, rows }),
};

// ── Browser (M13) ─────────────────────────────────────────────────────────

export type ConsoleEntry = {
  seq: number;
  ts: number;
  level: "error" | "warning" | "info" | "log" | "debug";
  source: "console" | "exception" | "network" | "log";
  text: string;
  url: string | null;
};

export type BrowserTab = {
  id: string;
  workspaceId: string;
  url: string;
  title: string;
  loading: boolean;
  width: number;
  height: number;
  mobile: boolean;
  createdAt: number;
};

export type BrowserEvent =
  | { type: "frame"; data: string; width: number; height: number }
  | { type: "nav"; url: string; title: string; loading: boolean }
  | { type: "console"; entry: ConsoleEntry }
  | { type: "closed"; reason: string };

export type BrowserAttach = { info: BrowserTab; entries: ConsoleEntry[]; token: number };
export type BrowserStatus = { available: boolean; name: string | null; path: string | null; running: boolean; version: string | null; tabs: number };
export type NavState = { canGoBack: boolean; canGoForward: boolean };
export type Shot = { path: string; width: number; height: number; bytes: number };
export type VerifyReport = {
  url: string;
  title: string;
  ok: boolean;
  loaded: boolean;
  elapsedMs: number;
  errors: ConsoleEntry[];
  warnings: ConsoleEntry[];
  screenshot: Shot | null;
  summary: string;
};
export type MouseInput = {
  kind: "mousePressed" | "mouseReleased" | "mouseMoved" | "mouseWheel";
  x: number;
  y: number;
  button?: "none" | "left" | "middle" | "right";
  clickCount?: number;
  deltaX?: number;
  deltaY?: number;
  modifiers?: number;
};
export type KeyInput = { kind: "keyDown" | "keyUp" | "rawKeyDown" | "char"; key: string; code?: string; text?: string; keyCode?: number; modifiers?: number };
export type DevScript = { name: string; command: string; server: boolean };
export type PreviewInfo = { hasPackageJson: boolean; packageManager: string; scripts: DevScript[] };

// ── Memory (M14) ──────────────────────────────────────────────────────────

export type MemoryScope = "global" | "workspace" | "mission" | "agent";
export type MemoryKind = "decision" | "convention" | "context" | "history" | "error" | "preference" | "doc";
export type MemoryStatus = "active" | "suggested" | "archived";
export type Memory = {
  id: string;
  scope: MemoryScope;
  workspaceId: string | null;
  missionId: string | null;
  profileId: string | null;
  kind: MemoryKind;
  title: string;
  body: string;
  tags: string[];
  pinned: boolean;
  status: MemoryStatus;
  source: "manual" | "agent";
  sourceRunId: string | null;
  createdAt: number;
  updatedAt: number;
};
export type MemoryInput = {
  scope: MemoryScope;
  workspaceId: string | null;
  missionId: string | null;
  profileId: string | null;
  kind: MemoryKind;
  title: string;
  body: string;
  tags: string[];
  pinned: boolean;
};
export type MemoryFilter = {
  scope?: MemoryScope | null;
  workspaceId?: string | null;
  missionId?: string | null;
  profileId?: string | null;
  kind?: MemoryKind | null;
  status?: MemoryStatus | null;
  query?: string | null;
  limit?: number | null;
};
export type MemoryCounts = { active: number; suggested: number; archived: number };
export type MemoryPreview = { labels: string[]; dropped: string[]; bytes: number };

// ── Voice (M15) ───────────────────────────────────────────────────────────

export type PermissionState = "authorized" | "denied" | "restricted" | "notDetermined" | "unknown";
export type VoiceStatus = { supported: boolean; helper: string | null; speech: PermissionState; microphone: PermissionState; listening: boolean; tts: boolean };
export type VoiceLocale = { id: string; onDevice: boolean };
export type VoiceEvent =
  | { type: "listening"; locale: string; onDevice: boolean }
  | { type: "partial"; text: string }
  | { type: "final"; text: string }
  | { type: "level"; rms: number }
  | { type: "ended"; reason: string }
  | { type: "error"; code: string; message: string }
  | { type: "exit"; code: number | null };
export type PauseTarget = { id: string; label: string };
export type PausePlan = { missions: PauseTarget[]; runs: PauseTarget[] };
export type VoiceSelfTest = { phrase: string; transcript: string; recall: number; ok: boolean; elapsedMs: number };

// ── Harness (M16) ─────────────────────────────────────────────────────────

export type HarnessPreference = "economy" | "balanced" | "quality";
export type Classification = {
  activity: "question" | "review" | "plan" | "docs" | "tests" | "bugfix" | "refactor" | "feature" | "design";
  domain: string | null;
  complexity: "small" | "medium" | "large";
  writes: boolean;
  signals: string[];
};
export type HarnessPick = { id: string; name: string };
export type Recommendation = {
  classification: Classification;
  profile: HarnessPick | null;
  providerId: ProviderId | null;
  model: string | null;
  permission: Permission;
  maxTurns: number;
  useWorktree: boolean;
  skills: HarnessPick[];
  mcp: HarnessPick[];
  reasons: string[];
  warnings: string[];
};
export type UsageStat = {
  providerId: string;
  model: string | null;
  runs: number;
  completed: number;
  avgDurationMs: number | null;
  costRuns: number;
  avgCostUsd: number | null;
};

// ── Media (M17) ───────────────────────────────────────────────────────────

export type MediaOp = "generate-image" | "analyze-image" | "tts" | "transcribe" | "video";
export type MediaAsset = {
  id: string;
  workspaceId: string | null;
  op: MediaOp | "screenshot";
  kind: "image" | "audio" | "video" | "text";
  provider: "gemini" | "claude" | "macos" | "browser";
  model: string | null;
  prompt: string;
  path: string | null;
  sourcePath: string | null;
  mime: string | null;
  bytes: number | null;
  resultText: string | null;
  status: "running" | "done" | "failed";
  error: string | null;
  createdAt: number;
  updatedAt: number;
};
export type GeminiModel = { id: string; displayName: string; methods: string[] };
export type GeminiStatus = {
  keySet: boolean;
  state: { models: GeminiModel[]; choices: Record<string, string>; lastCheckAt: number | null; lastOk: boolean | null; lastError: string | null };
  modelsByOp: Record<MediaOp, string | null>;
  candidatesByOp: Record<MediaOp, string[]>;
};
export type MediaStatus = { gemini: GeminiStatus; claudeInstalled: boolean; macosSpeech: PermissionState | null };
export type MediaInput = { type: "picked"; token: string } | { type: "asset"; id: string };

// ── Autopilot (M18) ───────────────────────────────────────────────────────

export type RoutineAction =
  | { type: "agent"; profileId: string; prompt: string; permission: "read" | "edit"; useWorktree: boolean }
  | { type: "script"; script: string }
  | { type: "command"; command: string }
  | { type: "mission"; recipeId: string; title: string; values: Record<string, string> }
  | { type: "verify"; url: string };
export type RoutineTrigger = "manual" | "interval" | "daily" | "commit";
export type Routine = {
  id: string;
  name: string;
  enabled: boolean;
  workspaceId: string;
  action: RoutineAction;
  trigger: RoutineTrigger;
  intervalMin: number | null;
  dailyTime: string | null;
  daysMask: number;
  maxRunsPerDay: number;
  timeoutMin: number;
  maxFailures: number;
  budgetUsdDay: number | null;
  commandApproved: boolean;
  consecutiveFailures: number;
  disabledReason: string | null;
  nextRunAt: number | null;
  createdAt: number;
  updatedAt: number;
};
export type RoutineInput = Omit<Routine, "id" | "enabled" | "commandApproved" | "consecutiveFailures" | "disabledReason" | "nextRunAt" | "createdAt" | "updatedAt"> & {
  approveCommand: boolean;
};
export type RoutineRun = {
  id: string;
  routineId: string;
  trigger: "schedule" | "manual" | "commit";
  status: "running" | "succeeded" | "failed" | "skipped" | "cancelled";
  output: string | null;
  error: string | null;
  agentRunId: string | null;
  missionId: string | null;
  costUsd: number | null;
  startedAt: number;
  endedAt: number | null;
};
export type AutopilotEvent = { routineId: string; routineName: string; runId: string; status: RoutineRun["status"]; message: string | null; autoDisabled: boolean };

// ── Observability (M19) ───────────────────────────────────────────────────

export type Reported<T> = { value: T | null; reportedRuns: number; unreportedRuns: number };
export type ProviderMetrics = {
  providerId: string;
  model: string | null;
  runs: number;
  completed: number;
  failed: number;
  cancelled: number;
  avgDurationMs: number | null;
  inputTokens: Reported<number>;
  outputTokens: Reported<number>;
  costUsd: Reported<number>;
};
export type DayPoint = { day: number; runs: number; failed: number; costUsd: number | null; costRuns: number };
export type Metrics = {
  since: number;
  runs: number;
  running: number;
  completed: number;
  failed: number;
  cancelled: number;
  p50DurationMs: number | null;
  p95DurationMs: number | null;
  inputTokens: Reported<number>;
  outputTokens: Reported<number>;
  cacheReadTokens: Reported<number>;
  costUsd: Reported<number>;
  providers: ProviderMetrics[];
  missions: Record<string, number>;
  tasks: Record<string, number>;
  routineRuns: Record<string, number>;
  routinesDisabled: number;
  media: Record<string, number>;
  toolUses: number;
  days: DayPoint[];
};
export type TimelineSource = "activity" | "agent" | "tool" | "mission" | "routine" | "media";
export type TimelineItem = {
  ts: number;
  source: TimelineSource;
  kind: string;
  level: "info" | "success" | "warning" | "error";
  title: string;
  detail: string | null;
  workspaceId: string | null;
  runId: string | null;
  missionId: string | null;
  routineId: string | null;
};
export type TimelineQuery = {
  query?: string | null;
  sources?: TimelineSource[] | null;
  workspaceId?: string | null;
  level?: string | null;
  since?: number | null;
  until?: number | null;
  limit?: number | null;
  audit?: boolean;
};
export type ReplayStep = { seq: number; offsetMs: number; kind: string; text: string };
export type ProcessInfo = {
  kind: "app" | "terminal" | "agent" | "browser" | "voice" | "routine";
  label: string;
  workspaceId: string | null;
  pid: number | null;
  startedAt: number | null;
  cpuPercent: number | null;
  rssKb: number | null;
  refId: string | null;
};

// ── App lifecycle (M20) ───────────────────────────────────────────────────

export type UpdateInfo = { current: string; available: boolean; version: string | null; notes: string | null; date: string | null };
export type CrashReport = { file: string; source: string; createdAt: number; summary: string; bytes: number };

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
