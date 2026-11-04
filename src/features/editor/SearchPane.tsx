import { useEffect, useMemo, useRef, useState } from "react";
import { CaseSensitive, ChevronRight, Loader2, Regex, Search, WholeWord } from "lucide-react";
import { EmptyState, Tooltip } from "@/components/ui";
import { findPane, setPaneState } from "@/features/panes/layout";
import type { PaneProps } from "@/features/panes/registry";
import { useLayout } from "@/features/panes/store";
import { api, errorMessage, type SearchMatch, type SearchResult } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { openFileInEditor } from "./actions";

type SearchState = { query: string; caseSensitive: boolean; regex: boolean; wholeWord: boolean };

function readSearchState(raw: Record<string, unknown> | undefined): SearchState {
  return {
    query: typeof raw?.query === "string" ? raw.query : "",
    caseSensitive: raw?.caseSensitive === true,
    regex: raw?.regex === true,
    wholeWord: raw?.wholeWord === true,
  };
}

function Toggle({ on, label, onClick, children }: { on: boolean; label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={on}
        onClick={onClick}
        className={cn("flex size-6 items-center justify-center rounded", on ? "bg-accent/20 text-accent" : "text-subtle hover:bg-elevated hover:text-fg")}
      >
        {children}
      </button>
    </Tooltip>
  );
}

function Preview({ m }: { m: SearchMatch }) {
  return (
    <span className="truncate font-mono text-[11.5px] text-muted">
      {m.preview.slice(0, m.previewStart)}
      <mark className="rounded-sm bg-accent/25 text-fg">{m.preview.slice(m.previewStart, m.previewEnd)}</mark>
      {m.preview.slice(m.previewEnd)}
    </span>
  );
}

export function SearchPane({ pane, workspaceId }: PaneProps) {
  const apply = useLayout((s) => s.apply);
  const focused = useLayout((s) => s.doc?.focusedId === pane.id);
  const opts = readSearchState(pane.state);
  const [input, setInput] = useState(opts.query);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const reqId = useRef(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const update = (patch: Partial<SearchState>) =>
    apply((d) => {
      const p = findPane(d, pane.id);
      return p ? setPaneState(d, pane.id, { ...(p.state ?? {}), ...readSearchState(p.state), ...patch }) : d;
    });

  useEffect(() => {
    if (focused) inputRef.current?.focus();
  }, [focused]);

  // Debounced search; stale responses are ignored.
  useEffect(() => {
    const q = input;
    const t = setTimeout(() => {
      if (q !== opts.query) update({ query: q });
      if (!q) {
        setResult(null);
        setError(null);
        return;
      }
      const id = ++reqId.current;
      setLoading(true);
      api
        .fsSearch(workspaceId, { query: q, caseSensitive: opts.caseSensitive, regex: opts.regex, wholeWord: opts.wholeWord })
        .then((r) => {
          if (id !== reqId.current) return;
          setResult(r);
          setError(null);
        })
        .catch((e) => id === reqId.current && (setError(errorMessage(e)), setResult(null)))
        .finally(() => id === reqId.current && setLoading(false));
    }, 250);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [input, opts.caseSensitive, opts.regex, opts.wholeWord, workspaceId]);

  const groups = useMemo(() => {
    const map = new Map<string, SearchMatch[]>();
    for (const m of result?.matches ?? []) {
      const list = map.get(m.relPath);
      if (list) list.push(m);
      else map.set(m.relPath, [m]);
    }
    return [...map.entries()];
  }, [result]);

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="shrink-0 space-y-1.5 border-b border-border p-2">
        <div className="flex h-8 items-center gap-1 rounded-md border border-border bg-bg pr-1 pl-2 focus-within:border-accent/60">
          <Search className="size-3.5 shrink-0 text-subtle" />
          <input
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Buscar no workspace"
            aria-label="Buscar no workspace"
            className="min-w-0 flex-1 bg-transparent text-[12.5px] text-fg placeholder:text-subtle focus:outline-none"
          />
          {loading && <Loader2 className="size-3.5 shrink-0 animate-spin text-subtle" />}
          <Toggle on={opts.caseSensitive} label="Diferenciar maiúsculas" onClick={() => update({ caseSensitive: !opts.caseSensitive })}>
            <CaseSensitive className="size-4" />
          </Toggle>
          <Toggle on={opts.wholeWord} label="Palavra inteira" onClick={() => update({ wholeWord: !opts.wholeWord })}>
            <WholeWord className="size-4" />
          </Toggle>
          <Toggle on={opts.regex} label="Expressão regular" onClick={() => update({ regex: !opts.regex })}>
            <Regex className="size-3.5" />
          </Toggle>
        </div>
        {result && (
          <div className="px-0.5 text-[11px] text-subtle">
            {result.matches.length === 0
              ? `Nenhum resultado em ${result.filesScanned} arquivos`
              : `${result.matches.length}${result.truncated ? "+" : ""} resultados em ${result.filesWithMatches} arquivos`}
            {result.truncated && " — refine a busca para ver todos"}
          </div>
        )}
        {error && <div className="px-0.5 text-[11px] text-error">{error}</div>}
      </div>
      <div className="min-h-0 flex-1 overflow-auto py-1">
        {!input && <EmptyState icon={<Search />} title="Busca no workspace" description="Respeita o .gitignore e ignora binários, .git e node_modules." />}
        {groups.map(([rel, matches]) => {
          const isCollapsed = collapsed.has(rel);
          return (
            <div key={rel}>
              <button
                type="button"
                className="flex h-6 w-full items-center gap-1 px-2 text-left text-[12px] hover:bg-elevated"
                onClick={() =>
                  setCollapsed((c) => {
                    const n = new Set(c);
                    if (n.has(rel)) n.delete(rel);
                    else n.add(rel);
                    return n;
                  })
                }
              >
                <ChevronRight className={cn("size-3 shrink-0 text-subtle transition-transform", !isCollapsed && "rotate-90")} />
                <span className="truncate text-fg">{rel.slice(rel.lastIndexOf("/") + 1)}</span>
                <span className="truncate text-[11px] text-subtle">{rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : ""}</span>
                <span className="ml-auto shrink-0 rounded bg-elevated px-1.5 text-[10.5px] text-muted">{matches.length}</span>
              </button>
              {!isCollapsed &&
                matches.map((m) => (
                  <button
                    key={`${m.line}:${m.column}`}
                    type="button"
                    onClick={() => openFileInEditor(m.relPath, { line: m.line, column: m.column, length: m.length })}
                    className="flex h-6 w-full items-center gap-2 pr-2 pl-7 text-left hover:bg-elevated"
                    title={`${m.relPath}:${m.line}:${m.column}`}
                  >
                    <span className="w-8 shrink-0 text-right font-mono text-[10.5px] text-subtle">{m.line}</span>
                    <Preview m={m} />
                  </button>
                ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
