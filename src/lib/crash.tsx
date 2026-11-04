import React from "react";
import { api, isDesktop } from "./ipc";

let lastKey = "";
let lastAt = 0;

/** Sends an unhandled UI error to the local crash log (deduplicated, never remote). */
export function reportUiError(summary: string, details: string) {
  const key = `${summary}\n${details.slice(0, 200)}`;
  const now = Date.now();
  if (key === lastKey && now - lastAt < 5000) return;
  lastKey = key;
  lastAt = now;
  if (isDesktop()) void api.crashReportUi(summary, `${details}\n\nURL: ${location.hash || location.pathname}\nUA: ${navigator.userAgent}`).catch(() => undefined);
}

export function installUiErrorHandlers() {
  window.addEventListener("error", (e) => {
    reportUiError(`${e.message || "Erro"}`, `${e.filename ?? ""}:${e.lineno ?? 0}:${e.colno ?? 0}\n${(e.error as Error | undefined)?.stack ?? ""}`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    const r = e.reason as unknown;
    // Backend errors handled by the UI are not crashes; only real exceptions are.
    if (r instanceof Error) reportUiError(`Promise rejeitada: ${r.message}`, r.stack ?? "");
  });
}

type State = { error: Error | null };

/** Last-resort boundary: records the error and offers a reload instead of a blank window. */
export class CrashBoundary extends React.Component<{ children: React.ReactNode }, State> {
  state: State = { error: null };
  static getDerivedStateFromError(error: Error): State {
    return { error };
  }
  componentDidCatch(error: Error, info: React.ErrorInfo) {
    reportUiError(`Falha na interface: ${error.message}`, `${error.stack ?? ""}\n\nComponentes:${info.componentStack ?? ""}`);
  }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-3 bg-bg p-8 text-center">
        <p className="text-[15px] font-semibold text-fg">Algo deu errado na interface do FORGE</p>
        <p className="max-w-md text-[12.5px] text-muted">
          O erro foi registrado localmente em Configurações → Sobre → Relatórios de falha (nada foi enviado). Seus dados, terminais e agentes continuam no núcleo.
        </p>
        <pre className="max-w-xl truncate font-mono text-[11px] text-subtle">{this.state.error.message}</pre>
        <button className="rounded-md bg-accent px-3 py-1.5 text-[12.5px] font-medium text-[#05080d]" onClick={() => location.reload()}>
          Recarregar a interface
        </button>
      </div>
    );
  }
}
