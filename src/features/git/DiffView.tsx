import { useEffect, useRef, useState } from "react";
import { Columns2, Loader2, Rows2 } from "lucide-react";
import { Button, EmptyState, Tooltip } from "@/components/ui";
import { monaco } from "@/features/editor/monaco";
import { cn } from "@/lib/utils";
import type { LineChange, LineRange } from "./lineChanges";

export type DiffContext = {
  original: string;
  modified: string;
  changes: LineChange[];
  /** Selected line ranges in the modified (right) editor; falls back to the cursor line. */
  selection: LineRange[];
};
export type DiffAction = { label: string; variant?: "secondary" | "danger" | "ghost"; run: (ctx: DiffContext) => void };

const FONT = '"Geist Mono Variable", ui-monospace, "SF Mono", Menlo, Consolas, monospace';
let seq = 0;

export function DiffView({
  title,
  subtitle,
  original,
  modified,
  path,
  loading,
  notice,
  actions = [],
}: {
  title: string;
  subtitle?: string;
  original: string | null;
  modified: string | null;
  path: string;
  loading?: boolean;
  notice?: string | null;
  actions?: DiffAction[];
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<monaco.editor.IStandaloneDiffEditor | null>(null);
  const [inline, setInline] = useState(false);
  const [changeCount, setChangeCount] = useState<number | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const ed = monaco.editor.createDiffEditor(host, {
      theme: "forge-dark",
      readOnly: true,
      originalEditable: false,
      automaticLayout: true,
      renderSideBySide: true,
      fontFamily: FONT,
      fontSize: 12.5,
      lineHeight: 19,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      renderOverviewRuler: true,
      ignoreTrimWhitespace: false,
      hideUnchangedRegions: { enabled: true, contextLineCount: 3, minimumLineCount: 6 },
    });
    editorRef.current = ed;
    const sub = ed.onDidUpdateDiff(() => setChangeCount(ed.getLineChanges()?.length ?? 0));
    return () => {
      sub.dispose();
      const model = ed.getModel();
      ed.dispose();
      model?.original.dispose();
      model?.modified.dispose();
      editorRef.current = null;
    };
  }, []);

  useEffect(() => {
    const ed = editorRef.current;
    if (!ed || original === null || modified === null) return;
    const id = ++seq;
    const old = ed.getModel();
    const uri = (side: string) => monaco.Uri.from({ scheme: "forge-diff", path: `/${id}/${side}/${path}` });
    ed.setModel({
      original: monaco.editor.createModel(original, undefined, uri("a")),
      modified: monaco.editor.createModel(modified, undefined, uri("b")),
    });
    setChangeCount(null);
    old?.original.dispose();
    old?.modified.dispose();
  }, [original, modified, path]);

  useEffect(() => {
    editorRef.current?.updateOptions({ renderSideBySide: !inline });
  }, [inline]);

  const context = (): DiffContext | null => {
    const ed = editorRef.current;
    const model = ed?.getModel();
    if (!ed || !model) return null;
    const changes = (ed.getLineChanges() ?? []) as LineChange[];
    const mod = ed.getModifiedEditor();
    const sels = mod.getSelections() ?? [];
    const selection = sels.map((s) => ({ start: s.startLineNumber, end: s.endColumn === 1 && s.endLineNumber > s.startLineNumber ? s.endLineNumber - 1 : s.endLineNumber }));
    return { original: model.original.getValue(), modified: model.modified.getValue(), changes, selection };
  };

  const empty = original === null || modified === null;

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3">
        <span className="truncate text-[12px] font-medium text-fg">{title}</span>
        {subtitle && <span className="truncate text-[11px] text-subtle">{subtitle}</span>}
        {changeCount !== null && <span className="shrink-0 text-[11px] text-subtle">· {changeCount} bloco(s)</span>}
        <div className="ml-auto flex shrink-0 items-center gap-1">
          {actions.map((a) => (
            <Button
              key={a.label}
              size="sm"
              variant={a.variant ?? "secondary"}
              className="h-6"
              disabled={empty || !changeCount}
              onClick={() => {
                const ctx = context();
                if (ctx) a.run(ctx);
              }}
            >
              {a.label}
            </Button>
          ))}
          <Tooltip content={inline ? "Lado a lado" : "Em linha"}>
            <Button size="icon" variant="ghost" className="size-6" aria-label="Alternar modo do diff" onClick={() => setInline((v) => !v)}>
              {inline ? <Columns2 /> : <Rows2 />}
            </Button>
          </Tooltip>
        </div>
      </div>
      {notice && <div className="shrink-0 border-b border-border bg-warning/10 px-3 py-1 text-[11.5px] text-warning">{notice}</div>}
      <div className="relative min-h-0 flex-1">
        <div ref={hostRef} className={cn("absolute inset-0", empty && "invisible")} />
        {loading && (
          <div className="absolute inset-0 flex items-center justify-center bg-bg-2/60 text-subtle">
            <Loader2 className="size-4 animate-spin" />
          </div>
        )}
        {empty && !loading && <EmptyState className="absolute inset-0" title="Sem conteúdo para comparar" />}
      </div>
    </div>
  );
}
