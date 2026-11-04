import { useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import { Loader2, RotateCcw, SquareTerminal, TriangleAlert } from "lucide-react";
import { Button, EmptyState, StatusDot } from "@/components/ui";
import { api, errorMessage, IpcError, type TermEvent, type TerminalInfo } from "@/lib/ipc";
import { prettyPath } from "@/lib/utils";
import { setPaneState } from "../layout";
import type { PaneProps } from "../registry";
import { useLayout } from "../store";

const THEME: ITheme = {
  background: "#0d1015",
  foreground: "#e8edf5",
  cursor: "#579bff",
  cursorAccent: "#0d1015",
  selectionBackground: "#579bff40",
  black: "#1b2029",
  red: "#f16f78",
  green: "#35c98a",
  yellow: "#e9ae52",
  blue: "#579bff",
  magenta: "#a68bff",
  cyan: "#5cc8e0",
  white: "#c9d1de",
  brightBlack: "#5f6a7b",
  brightRed: "#ff8f97",
  brightGreen: "#5be0a6",
  brightYellow: "#f5c578",
  brightBlue: "#7fb3ff",
  brightMagenta: "#bfa8ff",
  brightCyan: "#86dcef",
  brightWhite: "#ffffff",
};

const FONT = '"Geist Mono Variable", ui-monospace, "SF Mono", Menlo, Consolas, monospace';
const WRITE_CHUNK = 32 * 1024;

type Status =
  | { kind: "starting" }
  | { kind: "running"; info: TerminalInfo }
  | { kind: "exited"; info: TerminalInfo; code: number | null }
  | { kind: "lost" }
  | { kind: "error"; message: string };

function shellName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

export type TerminalLaunch = { providerId: string; mode: "agent" | "login"; model: string | null };

export function TerminalPane({
  pane,
  workspaceId,
  launch,
  restartSignal,
}: PaneProps & { launch?: TerminalLaunch; restartSignal?: number }) {
  const launchRef = useRef(launch);
  launchRef.current = launch;
  const container = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const stateRef = useRef(pane.state);
  stateRef.current = pane.state;
  const apply = useLayout((s) => s.apply);
  const focused = useLayout((s) => s.doc?.focusedId === pane.id);
  const [status, setStatus] = useState<Status>({ kind: "starting" });
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const term = new Terminal({
      fontFamily: FONT,
      fontSize: 12.5,
      lineHeight: 1.25,
      cursorBlink: true,
      cursorStyle: "bar",
      allowProposedApi: false,
      scrollback: 10_000,
      theme: THEME,
      macOptionIsMeta: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(
      new WebLinksAddon((event, uri) => {
        event.preventDefault();
        void openUrl(uri).catch(() => undefined);
      }),
    );
    term.open(el);
    termRef.current = term;
    const safeFit = () => {
      try {
        if (el.clientWidth > 0 && el.clientHeight > 0) fit.fit();
      } catch {
        /* element not measurable yet */
      }
    };
    safeFit();
    void document.fonts?.ready.then(() => {
      term.options.fontFamily = FONT;
      safeFit();
    });

    let disposed = false;
    let id: string | null = typeof stateRef.current?.terminalId === "string" ? stateRef.current.terminalId : null;
    let token: number | null = null;

    const channel = new Channel<TermEvent>();
    channel.onmessage = (ev) => {
      if (disposed) return;
      if (ev.type === "data") term.write(ev.data);
      else setStatus((s) => (s.kind === "running" ? { kind: "exited", info: s.info, code: ev.code } : s));
    };

    (async () => {
      try {
        if (!id) {
          const info = await api.terminalSpawn(workspaceId, null, term.cols, term.rows, launchRef.current ?? null);
          if (disposed) {
            void api.terminalKill(info.id).catch(() => undefined);
            return;
          }
          id = info.id;
          const newId = info.id;
          apply((d) => setPaneState(d, pane.id, { ...(stateRef.current ?? {}), terminalId: newId }));
        }
        const res = await api.terminalAttach(id, channel);
        if (disposed) {
          void api.terminalDetach(id, res.token).catch(() => undefined);
          return;
        }
        token = res.token;
        term.reset();
        if (res.scrollback) term.write(res.scrollback);
        setStatus(res.info.exited ? { kind: "exited", info: res.info, code: res.info.exitCode } : { kind: "running", info: res.info });
        if (!res.info.exited) await api.terminalResize(id, term.cols, term.rows);
      } catch (e) {
        if (disposed) return;
        setStatus(e instanceof IpcError && e.kind === "not_found" ? { kind: "lost" } : { kind: "error", message: errorMessage(e) });
      }
    })();

    const dataSub = term.onData((data) => {
      if (!id) return;
      for (let i = 0; i < data.length; i += WRITE_CHUNK) {
        void api.terminalWrite(id, data.slice(i, i + WRITE_CHUNK)).catch(() => undefined);
      }
    });

    let resizeTimer: ReturnType<typeof setTimeout> | null = null;
    const ro = new ResizeObserver(() => {
      safeFit();
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        if (id && token !== null) void api.terminalResize(id, term.cols, term.rows).catch(() => undefined);
      }, 60);
    });
    ro.observe(el);

    return () => {
      disposed = true;
      if (resizeTimer) clearTimeout(resizeTimer);
      ro.disconnect();
      dataSub.dispose();
      if (id && token !== null) void api.terminalDetach(id, token).catch(() => undefined);
      term.dispose();
      termRef.current = null;
    };
    // `generation` restarts the session; pane state is read through a ref on purpose.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId, pane.id, generation]);

  useEffect(() => {
    if (focused && status.kind === "running") termRef.current?.focus();
  }, [focused, status.kind]);

  const restart = () => {
    const old = typeof stateRef.current?.terminalId === "string" ? stateRef.current.terminalId : null;
    if (old) void api.terminalKill(old).catch(() => undefined);
    const { terminalId: _drop, ...rest } = stateRef.current ?? {};
    void _drop;
    stateRef.current = rest;
    apply((d) => setPaneState(d, pane.id, rest));
    setStatus({ kind: "starting" });
    setGeneration((g) => g + 1);
  };

  // External restart requests (agent mode/model changes).
  const firstSignal = useRef(restartSignal);
  useEffect(() => {
    if (restartSignal === undefined || restartSignal === firstSignal.current) return;
    firstSignal.current = restartSignal;
    restart();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [restartSignal]);

  const info = status.kind === "running" || status.kind === "exited" ? status.info : null;

  return (
    <div className="flex h-full flex-col bg-bg-2">
      <div className="relative min-h-0 flex-1">
        <div ref={container} className="absolute inset-0 py-1.5 pl-2" />
        {status.kind === "starting" && (
          <div className="absolute inset-0 flex items-center justify-center text-subtle">
            <Loader2 className="size-4 animate-spin" />
          </div>
        )}
        {(status.kind === "lost" || status.kind === "error") && (
          <div className="absolute inset-0 bg-bg-2">
            <EmptyState
              className="h-full"
              icon={status.kind === "lost" ? <SquareTerminal /> : <TriangleAlert />}
              title={status.kind === "lost" ? "Sessão de terminal encerrada" : "Não foi possível iniciar o terminal"}
              description={status.kind === "lost" ? "Terminais duram enquanto o FORGE está aberto. Esta sessão terminou quando o app foi fechado." : status.message}
              action={
                <Button size="sm" onClick={restart}>
                  <RotateCcw /> Novo terminal
                </Button>
              }
            />
          </div>
        )}
      </div>
      {status.kind === "exited" && (
        <div className="flex h-8 shrink-0 items-center gap-2 border-t border-border bg-surface px-3 text-xs text-muted">
          <StatusDot tone={status.code === 0 ? "neutral" : "warning"} />
          Processo encerrado{status.code !== null ? ` (código ${status.code})` : ""}
          <Button size="sm" variant="ghost" className="ml-auto" onClick={restart}>
            <RotateCcw /> Reiniciar
          </Button>
        </div>
      )}
      <div className="flex h-6 shrink-0 items-center gap-2 border-t border-border px-2.5 font-mono text-[10.5px] text-subtle">
        <StatusDot tone={status.kind === "running" ? "success" : status.kind === "exited" ? "neutral" : status.kind === "starting" ? "accent" : "error"} />
        {info ? (
          <>
            <span>{launch ? (launch.mode === "login" ? "login" : shellName(info.shell)) : shellName(info.shell)}</span>
            <span className="min-w-0 truncate" title={info.cwd}>
              {prettyPath(info.cwd)}
            </span>
            {info.pid !== null && <span className="ml-auto shrink-0">pid {info.pid}</span>}
          </>
        ) : (
          <span>{status.kind === "starting" ? "iniciando…" : "sem processo"}</span>
        )}
      </div>
    </div>
  );
}
