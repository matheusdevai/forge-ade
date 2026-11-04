import { useEffect, useRef, useState } from "react";
import { Bot, GitBranchPlus, Loader2, MessageSquarePlus, Send, ShieldAlert, Wand2, X } from "lucide-react";
import { HarnessPanel } from "@/features/harness/HarnessPanel";
import { toast } from "sonner";
import { Badge, Button, Dialog, DialogContent, EmptyState, Kbd, Tooltip } from "@/components/ui";
import { findPane, setPaneState } from "@/features/panes/layout";
import type { PaneProps } from "@/features/panes/registry";
import { useLayout } from "@/features/panes/store";
import { api, errorMessage, type AgentRun, type HarnessPreference, type Permission, type ProviderId, type Recommendation } from "@/lib/ipc";
import { cn, modKey } from "@/lib/utils";
import { PERMISSION_INFO, useAgents } from "@/stores/agents";
import { useProviders } from "@/stores/providers";
import { RunView } from "./RunView";

/** Per-conversation overrides applied from a Harness recommendation. */
type HarnessOverride = {
  providerId: ProviderId;
  model: string | null;
  maxTurns: number;
  extraSkillIds: string[];
  extraMcpIds: string[];
  summary: string;
};

type ThreadState = {
  profileId: string | null;
  permission: Permission | null;
  useWorktree: boolean;
  runIds: string[];
  harness: HarnessOverride | null;
  harnessPref: HarnessPreference;
};

function readHarness(raw: unknown): HarnessOverride | null {
  if (!raw || typeof raw !== "object") return null;
  const h = raw as Record<string, unknown>;
  if (typeof h.providerId !== "string" || typeof h.maxTurns !== "number") return null;
  const ids = (v: unknown) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
  return {
    providerId: h.providerId as ProviderId,
    model: typeof h.model === "string" ? h.model : null,
    maxTurns: h.maxTurns,
    extraSkillIds: ids(h.extraSkillIds),
    extraMcpIds: ids(h.extraMcpIds),
    summary: typeof h.summary === "string" ? h.summary : "",
  };
}

function readState(raw: Record<string, unknown> | undefined): ThreadState {
  const perm = raw?.permission;
  return {
    profileId: typeof raw?.profileId === "string" ? raw.profileId : null,
    permission: perm === "read" || perm === "edit" || perm === "full" ? perm : null,
    useWorktree: raw?.useWorktree === true,
    runIds: Array.isArray(raw?.runIds) ? (raw.runIds as unknown[]).filter((x): x is string => typeof x === "string") : [],
    harness: readHarness(raw?.harness),
    harnessPref: raw?.harnessPref === "economy" || raw?.harnessPref === "quality" ? raw.harnessPref : "balanced",
  };
}

export function AgentRunPane({ pane, workspaceId }: PaneProps) {
  const apply = useLayout((s) => s.apply);
  const state = readState(pane.state);
  const { profiles, loaded, refresh } = useAgents();
  const providers = useProviders((s) => s.items);
  const providersLoaded = useProviders((s) => s.loaded);
  const [runs, setRuns] = useState<Record<string, AgentRun>>({});
  const [prompt, setPrompt] = useState("");
  const [starting, setStarting] = useState(false);
  const [confirmFull, setConfirmFull] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const draft = typeof pane.state?.draft === "string" ? pane.state.draft : null;
  const sendAfterDraft = useRef(false);

  // A draft handed over by another pane (e.g. a browser verify report):
  // prefill the prompt once; the user still reviews and starts the run.
  useEffect(() => {
    if (!draft) return;
    setPrompt(draft);
    sendAfterDraft.current = pane.state?.draftSend === true;
    apply((d) => {
      const p = findPane(d, pane.id);
      if (!p) return d;
      const { draft: _consumed, draftSend: _send, ...rest } = p.state ?? {};
      return setPaneState(d, pane.id, rest);
    });
  }, [draft, apply, pane.id]);

  useEffect(() => {
    if (!loaded) void refresh().catch(() => undefined);
    if (!providersLoaded) void useProviders.getState().refresh().catch(() => undefined);
  }, [loaded, refresh, providersLoaded]);

  const update = (patch: Partial<ThreadState>) =>
    apply((d) => {
      const p = findPane(d, pane.id);
      return p ? setPaneState(d, pane.id, { ...(p.state ?? {}), ...patch }) : d;
    });

  const profile = profiles.find((p) => p.id === state.profileId) ?? profiles[0] ?? null;
  const permission: Permission = state.permission ?? profile?.permission ?? "read";
  const provider = providers.find((p) => p.id === profile?.providerId);
  const lastId = state.runIds[state.runIds.length - 1];
  const last = lastId ? runs[lastId] : undefined;
  const busy = starting || last?.status === "running";
  const providerId = (state.harness?.providerId ?? profile?.providerId) as ProviderId | undefined;
  const continuing = !!last && !!last.sessionId && last.providerId === providerId && last.providerId !== "gemini";
  const [harnessOpen, setHarnessOpen] = useState(false);

  const applyHarness = (r: Recommendation) => {
    if (!r.profile || !r.providerId) return;
    update({
      profileId: r.profile.id,
      permission: r.permission,
      useWorktree: state.runIds.length === 0 ? r.useWorktree : state.useWorktree,
      harness: {
        providerId: r.providerId,
        model: r.model,
        maxTurns: r.maxTurns,
        extraSkillIds: r.skills.map((x) => x.id),
        extraMcpIds: r.mcp.map((x) => x.id),
        summary: [r.providerId, r.model ?? "modelo padrão", `${r.maxTurns} turnos`, ...r.skills.map((x) => x.name), ...r.mcp.map((x) => `mcp ${x.name}`)].join(" · "),
      },
    });
    setHarnessOpen(false);
    toast.success("Configuração do Harness aplicada", { description: `${r.profile.name} · ${r.permission === "edit" ? "pode editar" : "somente leitura"}` });
  };

  // Keep the newest output in view.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const near = el.scrollHeight - el.scrollTop - el.clientHeight < 160;
    if (near) el.scrollTop = el.scrollHeight;
  });

  const start = async (confirmed = false) => {
    const text = prompt.trim();
    if (!text || !profile || busy) return;
    if (permission === "full" && !confirmed) {
      setConfirmFull(true);
      return;
    }
    setStarting(true);
    try {
      const run = await api.agentRunStart({
        workspaceId,
        profileId: profile.id,
        parentRunId: continuing ? last!.id : null,
        providerId: (state.harness?.providerId ?? profile.providerId) as ProviderId,
        model: state.harness ? state.harness.model : profile.model,
        permission,
        instructions: profile.instructions || null,
        maxTurns: state.harness?.maxTurns ?? profile.maxTurns,
        extraSkillIds: state.harness?.extraSkillIds ?? [],
        extraMcpIds: state.harness?.extraMcpIds ?? [],
        prompt: text,
        useWorktree: !continuing && state.runIds.length === 0 && state.useWorktree,
        confirmFull: permission === "full" && confirmed,
      });
      setRuns((r) => ({ ...r, [run.id]: run }));
      update({ runIds: [...state.runIds, run.id] });
      setPrompt("");
    } catch (e) {
      toast.error("Não foi possível iniciar o agente", { description: errorMessage(e) });
    } finally {
      setStarting(false);
    }
  };

  // Voice dictation with auto-send (Clock Voice): start once the draft is in.
  useEffect(() => {
    if (sendAfterDraft.current && prompt.trim() && profile && !busy) {
      sendAfterDraft.current = false;
      void start();
    }
  });

  if (!loaded) {
    return (
      <div className="flex h-full items-center justify-center text-subtle">
        <Loader2 className="size-4 animate-spin" />
      </div>
    );
  }

  const providerProblem = !provider
    ? null
    : !provider.installed
      ? `${provider.name} não está instalado`
      : provider.auth.state === "logged_out"
        ? `${provider.name} sem login — faça login em Providers`
        : null;

  return (
    <div className="flex h-full min-w-0 flex-col">
      {/* Config bar */}
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-bg px-2.5">
        <Bot className="size-3.5 shrink-0 text-accent" />
        <select
          value={profile?.id ?? ""}
          onChange={(e) => update({ profileId: e.target.value, permission: null, harness: null })}
          disabled={state.runIds.length > 0}
          aria-label="Perfil do agente"
          className="h-6 max-w-[200px] rounded border border-border bg-bg-2 px-1.5 text-[12px] text-fg focus:outline-none disabled:opacity-70"
          title={state.runIds.length > 0 ? "Inicie uma nova conversa para trocar o perfil" : undefined}
        >
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        {provider && <span className="truncate text-[11px] text-subtle">{provider.name}{profile?.model ? ` · ${profile.model}` : ""}</span>}
        <div className="ml-auto flex items-center gap-1.5">
          <select
            value={permission}
            onChange={(e) => update({ permission: e.target.value as Permission })}
            aria-label="Permissão"
            title={PERMISSION_INFO[permission].description}
            className={cn(
              "h-6 rounded border bg-bg-2 px-1.5 text-[11.5px] focus:outline-none",
              permission === "read" ? "border-border text-fg" : permission === "edit" ? "border-warning/40 text-warning" : "border-error/50 text-error",
            )}
          >
            {(Object.keys(PERMISSION_INFO) as Permission[]).map((p) => (
              <option key={p} value={p}>
                {PERMISSION_INFO[p].label}
              </option>
            ))}
          </select>
          <Tooltip content={state.runIds.length > 0 ? "Definido no início da conversa" : "Executar numa worktree isolada (nova branch), sem tocar no seu diretório de trabalho"}>
            <button
              type="button"
              disabled={state.runIds.length > 0}
              onClick={() => update({ useWorktree: !state.useWorktree })}
              aria-pressed={state.useWorktree}
              className={cn(
                "flex h-6 items-center gap-1 rounded border px-1.5 text-[11.5px] disabled:opacity-60",
                state.useWorktree ? "border-accent/50 bg-accent/10 text-accent" : "border-border text-muted",
              )}
            >
              <GitBranchPlus className="size-3" /> Worktree
            </button>
          </Tooltip>
          <Tooltip content="Nova conversa">
            <Button size="icon" variant="ghost" className="size-6" aria-label="Nova conversa" disabled={busy || state.runIds.length === 0} onClick={() => update({ runIds: [] })}>
              <MessageSquarePlus />
            </Button>
          </Tooltip>
        </div>
      </div>

      {/* Thread */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
        {state.runIds.length === 0 ? (
          <EmptyState
            className="h-full"
            icon={<Bot />}
            title={profile ? profile.name : "Nenhum perfil"}
            description={
              profile ? (
                <>
                  {profile.description}
                  <span className="mt-2 block text-subtle">
                    {PERMISSION_INFO[permission].description} {state.useWorktree ? "Roda numa worktree isolada." : "Roda no diretório do workspace."}
                  </span>
                </>
              ) : (
                "Crie um perfil em Agentes."
              )
            }
          />
        ) : (
          state.runIds.map((id) => <RunView key={id} runId={id} onUpdate={(r) => setRuns((m) => (m[r.id] === r ? m : { ...m, [r.id]: r }))} />)
        )}
      </div>

      {/* Composer */}
      <div className="shrink-0 border-t border-border p-2">
        {providerProblem && (
          <p className="mb-1.5 flex items-center gap-1.5 text-[11.5px] text-warning">
            <ShieldAlert className="size-3.5" /> {providerProblem}
          </p>
        )}
        {harnessOpen && prompt.trim() && (
          <HarnessPanel
            text={prompt}
            preference={state.harnessPref}
            onPreference={(p) => update({ harnessPref: p })}
            onApply={applyHarness}
            onClose={() => setHarnessOpen(false)}
          />
        )}
        <div className="flex items-end gap-2 rounded-lg border border-border bg-bg-2 p-1.5 focus-within:border-accent/50">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void start();
              }
            }}
            rows={2}
            placeholder={continuing ? "Continuar a conversa…" : `Peça algo ao ${profile?.name ?? "agente"}…`}
            aria-label="Mensagem para o agente"
            className="max-h-48 min-h-[40px] flex-1 resize-y bg-transparent px-1.5 py-1 text-[13px] text-fg placeholder:text-subtle focus:outline-none"
          />
          <Tooltip content="Harness: sugerir perfil, modelo, esforço, permissão, skills e MCP para este pedido">
            <Button size="icon" variant={harnessOpen ? "secondary" : "ghost"} className="h-8 w-8" disabled={!prompt.trim() || busy} onClick={() => setHarnessOpen((v) => !v)} aria-label="Harness">
              <Wand2 />
            </Button>
          </Tooltip>
          <Button size="sm" variant="primary" className="h-8" disabled={!prompt.trim() || !profile || busy} onClick={() => void start()}>
            {busy ? <Loader2 className="animate-spin" /> : <Send />} <Kbd className="border-transparent bg-black/20 text-[#05080d]">{modKey}↵</Kbd>
          </Button>
        </div>
        <div className="mt-1 flex items-center gap-2 px-1 text-[10.5px] text-subtle">
          {state.harness && (
            <span className="flex items-center gap-1 rounded border border-accent/30 bg-accent/10 px-1.5 text-accent" title="Configuração aplicada pelo Harness nesta conversa">
              <Wand2 className="size-3" /> {state.harness.summary}
              <button aria-label="Remover ajustes do Harness" className="hover:text-fg" onClick={() => update({ harness: null })}>
                <X className="size-3" />
              </button>
            </span>
          )}
          {continuing ? <Badge>continua a sessão</Badge> : state.runIds.length > 0 && last?.providerId === "gemini" ? <span>Gemini: cada mensagem inicia uma nova sessão</span> : null}
          <span className="ml-auto">Modo sem interface · eventos registrados com segredos mascarados</span>
        </div>
      </div>

      <Dialog open={confirmFull} onOpenChange={setConfirmFull}>
        <DialogContent title="Executar com acesso total?" description={profile?.name}>
          <div className="px-5 py-4 text-[13px] text-muted">
            O agente poderá executar <b className="text-fg">qualquer comando e alterar qualquer arquivo</b> que o seu usuário puder, sem pedir aprovação. Prefira{" "}
            <b className="text-fg">Pode editar</b> e, se possível, rode numa <b className="text-fg">worktree</b> isolada.
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setConfirmFull(false)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirmFull(false);
                  void start(true);
                }}
              >
                Executar com acesso total
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
