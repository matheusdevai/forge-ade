import { useCallback, useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  ArrowRight,
  Bot,
  Camera,
  ClipboardCopy,
  FolderOpen,
  Globe,
  Loader2,
  Play,
  RotateCw,
  ScanSearch,
  ScrollText,
  Square,
  Terminal as TerminalIcon,
  TriangleAlert,
} from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Dialog, DialogContent, EmptyState, StatusDot, Tooltip } from "@/components/ui";
import { findPane, setPaneState } from "@/features/panes/layout";
import { ensurePane } from "@/features/panes/ensure";
import type { PaneProps } from "@/features/panes/registry";
import { useLayout } from "@/features/panes/store";
import {
  api,
  errorMessage,
  IpcError,
  type BrowserEvent,
  type BrowserStatus,
  type BrowserTab,
  type ConsoleEntry,
  type DevScript,
  type NavState,
  type PreviewInfo,
  type TermEvent,
  type VerifyReport,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { agentDraft, extractLocalUrl, keyEvents, mapPoint, modifiersOf, mouseButton, VIEWPORTS, type ViewportId } from "./input";

const MAX_CONSOLE = 1000;

type Status =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "live" }
  | { kind: "gone"; reason: string }
  | { kind: "error"; message: string };

function usePaneState(paneId: string) {
  const apply = useLayout((s) => s.apply);
  return useCallback(
    (patch: Record<string, unknown>) =>
      apply((d) => {
        const p = findPane(d, paneId);
        return p ? setPaneState(d, paneId, { ...(p.state ?? {}), ...patch }) : d;
      }),
    [apply, paneId],
  );
}

const str = (v: unknown) => (typeof v === "string" ? v : null);

export function BrowserPane(props: PaneProps) {
  return <BrowserSurface {...props} />;
}

export function PreviewPane(props: PaneProps) {
  const surface = useRef<SurfaceHandle | null>(null);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <DevServerBar {...props} onUrl={(u) => surface.current?.open(u)} />
      <div className="min-h-0 flex-1">
        <BrowserSurface {...props} handleRef={surface} />
      </div>
    </div>
  );
}

type SurfaceHandle = { open: (url: string) => void };

function BrowserSurface({ pane, workspaceId, handleRef }: PaneProps & { handleRef?: React.MutableRefObject<SurfaceHandle | null> }) {
  const patch = usePaneState(pane.id);
  const tabId = str(pane.state?.tabId);
  const savedUrl = str(pane.state?.url);
  const viewportId = (str(pane.state?.viewport) as ViewportId | null) ?? "fit";
  const consoleOpen = pane.state?.console === true;

  const [status, setStatus] = useState<Status>(tabId ? { kind: "starting" } : { kind: "idle" });
  const [info, setInfo] = useState<BrowserTab | null>(null);
  const [address, setAddress] = useState(savedUrl ?? "");
  const [editing, setEditing] = useState(false);
  const [entries, setEntries] = useState<ConsoleEntry[]>([]);
  const [nav, setNav] = useState<NavState>({ canGoBack: false, canGoForward: false });
  const [frameSize, setFrameSize] = useState<{ w: number; h: number } | null>(null);
  const [browser, setBrowser] = useState<BrowserStatus | null>(null);
  const [report, setReport] = useState<VerifyReport | null>(null);
  const [verifying, setVerifying] = useState(false);
  const [shooting, setShooting] = useState(false);

  const imgRef = useRef<HTMLImageElement>(null);
  const screenRef = useRef<HTMLDivElement>(null);
  const sinkRef = useRef<HTMLTextAreaElement>(null);
  const frameRef = useRef<{ w: number; h: number }>({ w: 0, h: 0 });
  const tabRef = useRef<string | null>(tabId);
  tabRef.current = tabId;
  const moveRaf = useRef<number | null>(null);

  useEffect(() => {
    void api.browserStatus().then(setBrowser).catch(() => undefined);
  }, []);

  // ── Attach to the tab (stream frames, nav and console) ─────────────────
  useEffect(() => {
    if (!tabId) return;
    let alive = true;
    let token: number | null = null;
    const ch = new Channel<BrowserEvent>();
    ch.onmessage = (ev) => {
      if (!alive) return;
      if (ev.type === "frame") {
        if (imgRef.current) imgRef.current.src = `data:image/jpeg;base64,${ev.data}`;
        const f = frameRef.current;
        if (f.w !== ev.width || f.h !== ev.height) {
          frameRef.current = { w: ev.width, h: ev.height };
          setFrameSize({ w: ev.width, h: ev.height });
        }
      } else if (ev.type === "nav") {
        setInfo((i) => (i ? { ...i, url: ev.url, title: ev.title, loading: ev.loading } : i));
        if (!ev.loading) void api.browserNavState(tabId).then((n) => alive && setNav(n)).catch(() => undefined);
      } else if (ev.type === "console") {
        setEntries((e) => (e.length >= MAX_CONSOLE ? [...e.slice(-MAX_CONSOLE + 1), ev.entry] : [...e, ev.entry]));
      } else if (ev.type === "closed") {
        setStatus({ kind: "gone", reason: ev.reason });
      }
    };
    setStatus({ kind: "starting" });
    api
      .browserAttach(tabId, ch)
      .then((snap) => {
        if (!alive) {
          void api.browserDetach(tabId, snap.token).catch(() => undefined);
          return;
        }
        token = snap.token;
        setInfo(snap.info);
        setEntries(snap.entries);
        setStatus({ kind: "live" });
        void api.browserNavState(tabId).then((n) => alive && setNav(n)).catch(() => undefined);
      })
      .catch((e) => {
        if (!alive) return;
        if (e instanceof IpcError && e.kind === "NotFound") setStatus({ kind: "gone", reason: "closed" });
        else setStatus({ kind: "error", message: errorMessage(e) });
      });
    return () => {
      alive = false;
      if (token !== null) void api.browserDetach(tabId, token).catch(() => undefined);
    };
  }, [tabId]);

  // Keep the address bar in sync with navigation (unless the user is typing).
  useEffect(() => {
    if (!editing && info?.url) setAddress(info.url === "about:blank" ? "" : info.url);
  }, [info?.url, editing]);

  useEffect(() => {
    if (info?.url && info.url !== "about:blank" && info.url !== savedUrl) patch({ url: info.url });
  }, [info?.url, savedUrl, patch]);

  const targetSize = useCallback(() => {
    const vp = VIEWPORTS.find((v) => v.id === viewportId) ?? VIEWPORTS[0]!;
    if (vp.id !== "fit") return { w: vp.width, h: vp.height, mobile: vp.mobile };
    const r = screenRef.current?.getBoundingClientRect();
    return { w: Math.max(320, Math.floor(r?.width ?? 1024)), h: Math.max(240, Math.floor(r?.height ?? 700)), mobile: false };
  }, [viewportId]);

  const open = useCallback(
    async (raw: string) => {
      const url = raw.trim();
      if (!url) return;
      setEditing(false);
      try {
        if (tabRef.current && status.kind !== "gone") {
          const normalized = await api.browserNavigate(tabRef.current, url);
          setAddress(normalized);
          return;
        }
        setStatus({ kind: "starting" });
        setEntries([]);
        const { w, h, mobile } = targetSize();
        const tab = await api.browserOpen(workspaceId, url, w, h, mobile);
        setInfo(tab);
        patch({ tabId: tab.id, url: tab.url === "about:blank" ? url : tab.url });
        void api.browserStatus().then(setBrowser).catch(() => undefined);
      } catch (e) {
        if (!tabRef.current) setStatus({ kind: "error", message: errorMessage(e) });
        toast.error("Não foi possível abrir a página", { description: errorMessage(e) });
      }
    },
    [patch, status.kind, targetSize, workspaceId],
  );

  useEffect(() => {
    if (handleRef) handleRef.current = { open: (u) => void open(u) };
  }, [handleRef, open]);

  // ── Viewport: follow the pane size in "fit" mode, fixed presets otherwise ──
  useEffect(() => {
    const el = screenRef.current;
    if (!el || !tabId || status.kind !== "live") return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const applyVp = () => {
      const { w, h, mobile } = targetSize();
      void api
        .browserViewport(tabId, w, h, mobile)
        .then((i) => setInfo((cur) => (cur ? { ...cur, width: i.width, height: i.height, mobile: i.mobile } : i)))
        .catch(() => undefined);
    };
    applyVp();
    if (viewportId !== "fit") return;
    const ro = new ResizeObserver(() => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(applyVp, 250);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (timer) clearTimeout(timer);
    };
  }, [tabId, status.kind, viewportId, targetSize]);

  // ── Input forwarding ───────────────────────────────────────────────────
  const point = (e: { clientX: number; clientY: number }) => {
    const img = imgRef.current;
    if (!img) return null;
    return mapPoint(e.clientX, e.clientY, img.getBoundingClientRect(), frameRef.current.w, frameRef.current.h);
  };

  const sendMouse = (kind: "mousePressed" | "mouseReleased" | "mouseMoved", e: React.MouseEvent) => {
    if (!tabId || status.kind !== "live") return;
    const p = point(e);
    if (!p) return;
    void api
      .browserMouse(tabId, {
        kind,
        ...p,
        button: kind === "mouseMoved" ? (e.buttons & 1 ? "left" : "none") : mouseButton(e.button),
        clickCount: kind === "mouseMoved" ? 0 : e.detail || 1,
        modifiers: modifiersOf(e),
      })
      .catch(() => undefined);
  };

  useEffect(() => {
    const el = screenRef.current;
    if (!el || !tabId) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const img = imgRef.current;
      if (!img) return;
      const p = mapPoint(e.clientX, e.clientY, img.getBoundingClientRect(), frameRef.current.w, frameRef.current.h);
      if (!p) return;
      void api.browserMouse(tabId, { kind: "mouseWheel", ...p, deltaX: e.deltaX, deltaY: e.deltaY, modifiers: modifiersOf(e) }).catch(() => undefined);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [tabId]);

  const onKey = (type: "down" | "up", e: React.KeyboardEvent) => {
    if (!tabId || status.kind !== "live") return;
    const mod = e.metaKey || e.ctrlKey;
    // Paste goes through the paste event; other ⌘ shortcuts stay with FORGE.
    if (mod && (e.key === "v" || e.key === "V")) return;
    if (mod && !["a", "c", "x", "z"].includes(e.key.toLowerCase())) return;
    e.preventDefault();
    e.stopPropagation();
    for (const k of keyEvents(type, e)) void api.browserKey(tabId, k).catch(() => undefined);
  };

  // ── Actions ────────────────────────────────────────────────────────────
  const shoot = async (fullPage: boolean) => {
    if (!tabId) return;
    setShooting(true);
    try {
      const shot = await api.browserScreenshot(tabId, fullPage);
      toast.success("Captura salva", {
        description: `${shot.width}×${shot.height} · ${(shot.bytes / 1024).toFixed(0)} KB`,
        action: { label: "Revelar", onClick: () => void api.browserRevealShot(shot.path).catch(() => undefined) },
      });
    } catch (e) {
      toast.error("Falha na captura", { description: errorMessage(e) });
    } finally {
      setShooting(false);
    }
  };

  const verify = async () => {
    if (!tabId) return;
    setVerifying(true);
    try {
      setReport(await api.browserVerify(tabId));
    } catch (e) {
      toast.error("Falha ao verificar a página", { description: errorMessage(e) });
    } finally {
      setVerifying(false);
    }
  };

  const sendToAgent = (r: VerifyReport) => {
    const id = ensurePane("agent");
    if (!id) return;
    useLayout.getState().apply((d) => {
      const p = findPane(d, id);
      return p ? setPaneState(d, id, { ...(p.state ?? {}), draft: agentDraft(r.summary) }) : d;
    });
    setReport(null);
    toast.success("Relatório enviado ao painel do agente", { description: "Revise o pedido e escolha a permissão antes de executar." });
  };

  const reopen = () => {
    patch({ tabId: null });
    setStatus({ kind: "idle" });
    if (savedUrl) {
      tabRef.current = null;
      void open(savedUrl);
    }
  };

  const errors = entries.filter((e) => e.level === "error").length;
  const live = status.kind === "live";
  const vp = VIEWPORTS.find((v) => v.id === viewportId) ?? VIEWPORTS[0]!;

  return (
    <div className="flex h-full min-h-0 flex-col bg-bg2">
      {/* Toolbar */}
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-border px-1.5">
        <Tooltip content="Voltar">
          <Button variant="ghost" size="icon" disabled={!live || !nav.canGoBack} onClick={() => tabId && void api.browserGo(tabId, -1).catch(() => undefined)} aria-label="Voltar">
            <ArrowLeft />
          </Button>
        </Tooltip>
        <Tooltip content="Avançar">
          <Button variant="ghost" size="icon" disabled={!live || !nav.canGoForward} onClick={() => tabId && void api.browserGo(tabId, 1).catch(() => undefined)} aria-label="Avançar">
            <ArrowRight />
          </Button>
        </Tooltip>
        <Tooltip content="Recarregar (⇧ ignora o cache)">
          <Button
            variant="ghost"
            size="icon"
            disabled={!live}
            onClick={(e) => tabId && void api.browserReload(tabId, e.shiftKey).catch((err) => toast.error("Falha ao recarregar", { description: errorMessage(err) }))}
            aria-label="Recarregar"
          >
            {info?.loading ? <Loader2 className="animate-spin" /> : <RotateCw />}
          </Button>
        </Tooltip>
        <form
          className="flex min-w-0 flex-1"
          onSubmit={(e) => {
            e.preventDefault();
            void open(address);
          }}
        >
          <input
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            onFocus={(e) => {
              setEditing(true);
              e.currentTarget.select();
            }}
            onBlur={() => setEditing(false)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setAddress(info?.url && info.url !== "about:blank" ? info.url : "");
                e.currentTarget.blur();
              }
            }}
            spellCheck={false}
            placeholder="localhost:3000 ou https://…"
            className="h-7 min-w-0 flex-1 rounded-md border border-border bg-bg px-2.5 font-mono text-[12px] text-fg outline-none placeholder:text-subtle focus:border-accent/60"
            aria-label="Endereço"
          />
        </form>
        <select
          value={viewportId}
          onChange={(e) => patch({ viewport: e.target.value })}
          className="h-7 rounded-md border border-border bg-bg px-1.5 text-[11.5px] text-muted outline-none"
          aria-label="Viewport"
          title="Tamanho da página"
        >
          {VIEWPORTS.map((v) => (
            <option key={v.id} value={v.id}>
              {v.label}
            </option>
          ))}
        </select>
        <Tooltip content="Capturar a área visível (⇧ = página inteira)">
          <Button variant="ghost" size="icon" disabled={!live || shooting} onClick={(e) => void shoot(e.shiftKey)} aria-label="Capturar tela">
            {shooting ? <Loader2 className="animate-spin" /> : <Camera />}
          </Button>
        </Tooltip>
        <Tooltip content="Verificar página: recarrega e relata erros, falhas de rede e captura">
          <Button variant="ghost" size="sm" disabled={!live || verifying} onClick={() => void verify()}>
            {verifying ? <Loader2 className="animate-spin" /> : <ScanSearch />}
            Verificar
          </Button>
        </Tooltip>
        <Tooltip content="Console e rede">
          <Button variant={consoleOpen ? "secondary" : "ghost"} size="sm" onClick={() => patch({ console: !consoleOpen })} aria-label="Console">
            <ScrollText />
            {errors > 0 && <Badge tone="error">{errors}</Badge>}
          </Button>
        </Tooltip>
      </div>

      {/* Screen */}
      <div
        ref={screenRef}
        className="relative min-h-0 flex-1 overflow-hidden bg-[#0b0d11]"
        onMouseDown={(e) => {
          sinkRef.current?.focus();
          e.preventDefault();
          sendMouse("mousePressed", e);
        }}
        onMouseUp={(e) => sendMouse("mouseReleased", e)}
        onMouseMove={(e) => {
          if (moveRaf.current !== null) return;
          const { clientX, clientY, buttons, button, detail, altKey, ctrlKey, metaKey, shiftKey } = e;
          moveRaf.current = requestAnimationFrame(() => {
            moveRaf.current = null;
            sendMouse("mouseMoved", { clientX, clientY, buttons, button, detail, altKey, ctrlKey, metaKey, shiftKey } as React.MouseEvent);
          });
        }}
        onContextMenu={(e) => e.preventDefault()}
      >
        {tabId && status.kind !== "gone" && (
          <img
            ref={imgRef}
            alt={info?.title || "Página"}
            draggable={false}
            className={cn("absolute inset-0 h-full w-full object-contain select-none", !frameSize && "invisible")}
          />
        )}
        <textarea
          ref={sinkRef}
          aria-label="Entrada de teclado da página"
          className="pointer-events-none absolute top-0 left-0 h-px w-px opacity-0"
          onKeyDown={(e) => onKey("down", e)}
          onKeyUp={(e) => onKey("up", e)}
          onPaste={(e) => {
            const text = e.clipboardData.getData("text");
            e.preventDefault();
            if (tabId && text) void api.browserInsertText(tabId, text).catch(() => undefined);
          }}
          onChange={() => {
            if (sinkRef.current) sinkRef.current.value = "";
          }}
        />
        {status.kind === "idle" && (
          <div className="absolute inset-0 grid place-items-center">
            {browser && !browser.available ? (
              <EmptyState
                icon={<TriangleAlert />}
                title="Nenhum navegador compatível encontrado"
                description="O navegador do FORGE usa o Google Chrome, Chromium, Brave ou Edge já instalado nesta máquina (modo headless, perfil isolado). Instale um deles e reabra este painel."
              />
            ) : (
              <EmptyState
                icon={<Globe />}
                title="Navegador do FORGE"
                description={
                  <>
                    Digite um endereço acima. Roda {browser?.name ?? "o Chrome instalado"} em modo headless com um perfil isolado — sem os seus cookies ou
                    logins pessoais.
                  </>
                }
              />
            )}
          </div>
        )}
        {(status.kind === "starting" || (live && !frameSize)) && (
          <div className="absolute inset-0 grid place-items-center text-xs text-muted">
            <span className="flex items-center gap-2">
              <Loader2 className="size-4 animate-spin" /> Iniciando navegador…
            </span>
          </div>
        )}
        {status.kind === "gone" && (
          <div className="absolute inset-0 grid place-items-center">
            <EmptyState
              icon={<Globe />}
              title={status.reason === "crashed" ? "A página travou" : "Sessão do navegador encerrada"}
              description={savedUrl ? `Última página: ${savedUrl}` : "O navegador foi fechado (ex.: o FORGE foi reiniciado)."}
              action={
                savedUrl ? (
                  <Button size="sm" onClick={reopen}>
                    <RotateCw /> Reabrir
                  </Button>
                ) : undefined
              }
            />
          </div>
        )}
        {status.kind === "error" && (
          <div className="absolute inset-0 grid place-items-center">
            <EmptyState icon={<TriangleAlert />} title="Falha no navegador" description={status.message} />
          </div>
        )}
        {live && vp.id !== "fit" && frameSize && (
          <div className="pointer-events-none absolute right-2 bottom-2 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] text-muted">
            {frameSize.w}×{frameSize.h}
          </div>
        )}
      </div>

      {consoleOpen && <ConsolePanel entries={entries} onClear={() => tabId && void api.browserConsoleClear(tabId).then(() => setEntries([]))} />}

      <Dialog open={!!report} onOpenChange={(o) => !o && setReport(null)}>
        {report && (
          <DialogContent
            title={report.ok ? "Página sem erros" : `${report.errors.length} erro(s) encontrado(s)`}
            description={`${report.url} · ${(report.elapsedMs / 1000).toFixed(1)}s${report.loaded ? "" : " · carregamento não concluído"}`}
            className="w-[min(720px,calc(100vw-32px))]"
          >
            <div className="max-h-[55vh] space-y-3 overflow-auto px-5 py-4">
              {report.errors.length === 0 && report.warnings.length === 0 && (
                <p className="text-[13px] text-success">Nenhum erro de console, exceção ou falha de rede observado após recarregar.</p>
              )}
              {[...report.errors, ...report.warnings].map((e) => (
                <EntryRow key={`${e.seq}`} entry={e} />
              ))}
              {report.screenshot && (
                <button
                  className="flex items-center gap-1.5 text-xs text-muted hover:text-fg"
                  onClick={() => report.screenshot && void api.browserRevealShot(report.screenshot.path)}
                >
                  <FolderOpen className="size-3.5" /> Captura: {report.screenshot.width}×{report.screenshot.height}
                </button>
              )}
            </div>
            <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void navigator.clipboard.writeText(report.summary).then(() => toast.success("Relatório copiado"))}
              >
                <ClipboardCopy /> Copiar relatório
              </Button>
              <Button size="sm" variant="primary" disabled={report.ok} onClick={() => sendToAgent(report)}>
                <Bot /> Enviar ao agente para corrigir
              </Button>
            </div>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}

const LEVEL_TONE: Record<ConsoleEntry["level"], string> = {
  error: "text-error",
  warning: "text-warning",
  info: "text-accent",
  log: "text-fg",
  debug: "text-subtle",
};

function EntryRow({ entry }: { entry: ConsoleEntry }) {
  return (
    <div className="flex gap-2 border-b border-border/60 px-2 py-1 font-mono text-[11.5px] leading-[1.45]">
      <span className={cn("w-14 shrink-0 uppercase", LEVEL_TONE[entry.level])}>{entry.source === "network" ? "rede" : entry.source === "exception" ? "exceção" : entry.level}</span>
      <div className="min-w-0 flex-1">
        <div className={cn("break-words whitespace-pre-wrap", entry.level === "error" ? "text-error" : "text-fg")}>{entry.text}</div>
        {entry.url && <div className="truncate text-subtle">{entry.url}</div>}
      </div>
    </div>
  );
}

function ConsolePanel({ entries, onClear }: { entries: ConsoleEntry[]; onClear: () => void }) {
  const [filter, setFilter] = useState<"all" | "problems">("all");
  const listRef = useRef<HTMLDivElement>(null);
  const shown = filter === "all" ? entries : entries.filter((e) => e.level === "error" || e.level === "warning");
  useEffect(() => {
    const el = listRef.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 80) el.scrollTop = el.scrollHeight;
  }, [shown.length]);
  return (
    <div className="flex h-[200px] shrink-0 flex-col border-t border-border bg-bg">
      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-border px-2 text-[11px]">
        <span className="font-medium text-muted">Console</span>
        <span className="text-subtle">· {entries.length}</span>
        <div className="flex-1" />
        <button className={cn("rounded px-1.5 py-0.5", filter === "all" ? "bg-elevated text-fg" : "text-muted")} onClick={() => setFilter("all")}>
          Tudo
        </button>
        <button className={cn("rounded px-1.5 py-0.5", filter === "problems" ? "bg-elevated text-fg" : "text-muted")} onClick={() => setFilter("problems")}>
          Problemas
        </button>
        <button className="rounded px-1.5 py-0.5 text-muted hover:text-fg" onClick={onClear}>
          Limpar
        </button>
      </div>
      <div ref={listRef} className="min-h-0 flex-1 overflow-auto">
        {shown.length === 0 ? (
          <p className="px-3 py-3 text-xs text-subtle">Nada registrado ainda.</p>
        ) : (
          shown.map((e) => <EntryRow key={e.seq} entry={e} />)
        )}
      </div>
    </div>
  );
}

// ── Preview: dev server in a real terminal session ───────────────────────

const OUTPUT_CAP = 64 * 1024;
// eslint-disable-next-line no-control-regex
const stripAnsi = (s: string) => s.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]|\u001b\][^\u0007]*(\u0007|\u001b\\)/g, "");

function DevServerBar({ pane, workspaceId, onUrl }: PaneProps & { onUrl: (url: string) => void }) {
  const patch = usePaneState(pane.id);
  const terminalId = str(pane.state?.terminalId);
  const script = str(pane.state?.script);
  const [preview, setPreview] = useState<PreviewInfo | null>(null);
  const [state, setState] = useState<"idle" | "running" | "exited">(terminalId ? "running" : "idle");
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [output, setOutput] = useState("");
  const [showLog, setShowLog] = useState(false);
  const [detected, setDetected] = useState<string | null>(null);
  const onUrlRef = useRef(onUrl);
  onUrlRef.current = onUrl;
  const logRef = useRef<HTMLPreElement>(null);
  const freshStart = useRef(false);

  useEffect(() => {
    void api
      .previewDetect(workspaceId)
      .then(setPreview)
      .catch((e) => toast.error("Não foi possível ler o package.json", { description: errorMessage(e) }));
  }, [workspaceId]);

  useEffect(() => {
    if (!terminalId) return;
    let alive = true;
    let token: number | null = null;
    let found = false;
    let buf = "";
    const push = (chunk: string) => {
      buf = (buf + chunk).slice(-OUTPUT_CAP);
      setOutput(buf);
      if (!found) {
        const url = extractLocalUrl(buf);
        if (url) {
          found = true;
          setDetected(url);
          onUrlRef.current(url);
        }
      }
    };
    const ch = new Channel<TermEvent>();
    ch.onmessage = (ev) => {
      if (!alive) return;
      if (ev.type === "data") push(ev.data);
      else {
        setState("exited");
        setExitCode(ev.code);
      }
    };
    api
      .terminalAttach(terminalId, ch)
      .then((a) => {
        if (!alive) {
          void api.terminalDetach(terminalId, a.token).catch(() => undefined);
          return;
        }
        token = a.token;
        const url = extractLocalUrl(a.scrollback);
        found = !!url;
        if (url) {
          setDetected(url);
          // Only navigate for a server we just started (not on remount).
          if (freshStart.current) onUrlRef.current(url);
        }
        freshStart.current = false;
        buf = a.scrollback.slice(-OUTPUT_CAP);
        setOutput(buf);
        setState(a.info.exited ? "exited" : "running");
        setExitCode(a.info.exitCode);
      })
      .catch(() => {
        if (!alive) return;
        patch({ terminalId: null });
        setState("idle");
      });
    return () => {
      alive = false;
      if (token !== null) void api.terminalDetach(terminalId, token).catch(() => undefined);
    };
  }, [terminalId, patch]);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [output, showLog]);

  const servers = preview?.scripts ?? [];
  const chosen: DevScript | undefined = servers.find((s) => s.name === script) ?? servers.find((s) => s.server) ?? servers[0];

  const start = async () => {
    if (!chosen) return;
    try {
      if (terminalId) await api.terminalKill(terminalId).catch(() => undefined);
      setOutput("");
      setDetected(null);
      setExitCode(null);
      freshStart.current = true;
      const t = await api.previewStart(workspaceId, chosen.name, 120, 30);
      setState("running");
      patch({ terminalId: t.id, script: chosen.name });
    } catch (e) {
      toast.error("Não foi possível iniciar a prévia", { description: errorMessage(e) });
    }
  };

  const stop = async () => {
    if (!terminalId) return;
    await api.terminalKill(terminalId).catch(() => undefined);
    setState("exited");
  };

  return (
    <div className="shrink-0 border-b border-border bg-surface">
      <div className="flex h-9 items-center gap-2 px-2 text-xs">
        <TerminalIcon className="size-3.5 text-subtle" />
        {preview && !preview.hasPackageJson ? (
          <span className="text-muted">Sem package.json neste workspace — digite a URL do servidor no campo abaixo.</span>
        ) : servers.length === 0 ? (
          <span className="text-muted">{preview ? "Nenhum script no package.json." : "Lendo package.json…"}</span>
        ) : (
          <>
            <select
              value={chosen?.name ?? ""}
              onChange={(e) => patch({ script: e.target.value })}
              disabled={state === "running"}
              className="h-7 max-w-[200px] rounded-md border border-border bg-bg px-1.5 text-[11.5px] text-fg outline-none"
              aria-label="Script"
            >
              {servers.map((s) => (
                <option key={s.name} value={s.name}>
                  {s.name}
                  {s.server ? "" : " (outro)"}
                </option>
              ))}
            </select>
            <code className="hidden min-w-0 flex-1 truncate font-mono text-[11px] text-subtle md:block" title={chosen?.command}>
              {preview?.packageManager} run {chosen?.name} · {chosen?.command}
            </code>
            <div className="flex-1 md:hidden" />
            {state === "running" ? (
              <Button size="sm" variant="danger" onClick={() => void stop()}>
                <Square /> Parar
              </Button>
            ) : (
              <Button size="sm" variant="primary" onClick={() => void start()} disabled={!chosen}>
                <Play /> {state === "exited" ? "Reiniciar" : "Iniciar"}
              </Button>
            )}
          </>
        )}
        <span className="flex items-center gap-1.5 text-muted">
          <StatusDot tone={state === "running" ? (detected ? "success" : "warning") : state === "exited" ? "error" : "neutral"} pulse={state === "running" && !detected} />
          {state === "running" ? (detected ? detected : "aguardando URL…") : state === "exited" ? `encerrado${exitCode !== null ? ` (${exitCode})` : ""}` : "parado"}
        </span>
        {(state !== "idle" || output) && (
          <Button size="sm" variant={showLog ? "secondary" : "ghost"} onClick={() => setShowLog((v) => !v)}>
            Saída
          </Button>
        )}
      </div>
      {showLog && (
        <pre ref={logRef} className="max-h-40 overflow-auto border-t border-border bg-bg px-3 py-2 font-mono text-[11px] leading-[1.45] whitespace-pre-wrap text-muted">
          {stripAnsi(output) || "Sem saída ainda."}
        </pre>
      )}
    </div>
  );
}
