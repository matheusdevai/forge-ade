import { useEffect, useMemo, useRef, useState } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { File, Loader2 } from "lucide-react";
import { api, errorMessage } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { openFileInEditor } from "./actions";
import { fuzzyFilter, type FuzzyResult } from "./fuzzy";

const cache = new Map<string, { at: number; files: string[]; truncated: boolean }>();
const TTL = 15_000;

function Highlighted({ r }: { r: FuzzyResult }) {
  const set = new Set(r.positions);
  const base = r.path.lastIndexOf("/") + 1;
  const render = (from: number, to: number) =>
    [...r.path.slice(from, to)].map((ch, i) => (set.has(from + i) ? <b key={from + i} className="font-semibold text-accent">{ch}</b> : ch));
  return (
    <>
      <span className="truncate text-[13px] text-fg">{render(base, r.path.length)}</span>
      <span className="truncate text-[11.5px] text-subtle">{render(0, Math.max(0, base - 1))}</span>
    </>
  );
}

export function QuickOpen() {
  const open = useApp((s) => s.quickOpen);
  const setOpen = useApp((s) => s.setQuickOpen);
  const route = useApp((s) => s.route);
  const workspaceId = route.view === "workspace" ? route.id : null;
  const [query, setQuery] = useState("");
  const [files, setFiles] = useState<string[] | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open || !workspaceId) return;
    setQuery("");
    setSelected(0);
    setError(null);
    const hit = cache.get(workspaceId);
    if (hit && Date.now() - hit.at < TTL) {
      setFiles(hit.files);
      setTruncated(hit.truncated);
      return;
    }
    setFiles(hit?.files ?? null);
    api
      .fsListFiles(workspaceId)
      .then((l) => {
        cache.set(workspaceId, { at: Date.now(), files: l.files, truncated: l.truncated });
        setFiles(l.files);
        setTruncated(l.truncated);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [open, workspaceId]);

  const results = useMemo(() => (files ? fuzzyFilter(query, files, 60) : []), [files, query]);

  useEffect(() => setSelected(0), [query]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const choose = (r: FuzzyResult | undefined) => {
    if (!r) return;
    setOpen(false);
    openFileInEditor(r.path);
  };

  return (
    <DialogPrimitive.Root open={open && !!workspaceId} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/40" />
        <DialogPrimitive.Content className="fixed top-[12vh] left-1/2 z-50 w-[min(640px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden rounded-xl border border-border bg-surface shadow-2xl shadow-black/60">
          <DialogPrimitive.Title className="sr-only">Abrir arquivo</DialogPrimitive.Title>
          <DialogPrimitive.Description className="sr-only">Busque arquivos do workspace pelo nome</DialogPrimitive.Description>
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelected((s) => Math.min(s + 1, results.length - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelected((s) => Math.max(s - 1, 0));
              } else if (e.key === "Enter") {
                e.preventDefault();
                choose(results[selected]);
              }
            }}
            placeholder="Buscar arquivo pelo nome…"
            aria-label="Buscar arquivo pelo nome"
            className="h-12 w-full border-b border-border bg-transparent px-4 text-[14px] text-fg placeholder:text-subtle focus:outline-none"
          />
          <div ref={listRef} role="listbox" className="max-h-[52vh] overflow-auto p-1.5">
            {error && <div className="px-3 py-6 text-center text-xs text-error">{error}</div>}
            {!files && !error && (
              <div className="flex justify-center py-6 text-subtle">
                <Loader2 className="size-4 animate-spin" />
              </div>
            )}
            {files && results.length === 0 && <div className="py-6 text-center text-xs text-subtle">Nenhum arquivo encontrado.</div>}
            {results.map((r, i) => (
              <div
                key={r.path}
                data-index={i}
                role="option"
                aria-selected={i === selected}
                onMouseMove={() => setSelected(i)}
                onClick={() => choose(r)}
                className={cn("flex h-8 cursor-default items-center gap-2.5 rounded-md px-2.5", i === selected ? "bg-accent/15" : "")}
              >
                <File className="size-3.5 shrink-0 text-subtle" />
                <Highlighted r={r} />
              </div>
            ))}
          </div>
          {truncated && <div className="border-t border-border px-4 py-1.5 text-[11px] text-subtle">Mostrando os primeiros 50.000 arquivos.</div>}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

/** Invalidate the file list after create/rename/delete. */
export function invalidateFileList(workspaceId: string) {
  cache.delete(workspaceId);
}
