import { CheckCircle2, CircleDashed, Loader2, RefreshCw, TriangleAlert } from "lucide-react";
import { Badge, Button } from "@/components/ui";
import type { ToolCategory, ToolStatus } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useEnvironment } from "@/stores/data";

const CATEGORY_LABEL: Record<ToolCategory, string> = {
  core: "Essenciais",
  agent: "Agentes (CLIs)",
  local: "Modelos locais",
  runtime: "Runtime",
};

function ToolRow({ t }: { t: ToolStatus }) {
  return (
    <div className="flex items-center gap-3 px-3 py-2">
      {t.installed ? (
        <CheckCircle2 className="size-4 shrink-0 text-success" />
      ) : t.error ? (
        <TriangleAlert className="size-4 shrink-0 text-warning" />
      ) : (
        <CircleDashed className={cn("size-4 shrink-0", t.required ? "text-error" : "text-subtle")} />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 text-[13px]">
          {t.name}
          {t.required && !t.installed && <Badge tone="error">obrigatório</Badge>}
        </div>
        <div className="selectable truncate font-mono text-[11px] text-subtle" title={t.path ?? t.hint}>
          {t.installed ? t.path : (t.error ?? `Não encontrado · ${t.hint}`)}
        </div>
      </div>
      {t.version && <span className="font-mono text-[11.5px] text-muted">v{t.version}</span>}
    </div>
  );
}

export function EnvironmentTable({ compact }: { compact?: boolean }) {
  const { tools, scanning, scannedAt, scan } = useEnvironment();
  const groups = (["core", "agent", "local", "runtime"] as ToolCategory[]).map((c) => ({ c, items: tools.filter((t) => t.category === c) }));

  return (
    <div>
      <div className="mb-3 flex items-center gap-2">
        <span className="text-xs text-muted">
          {scanning ? "Verificando ferramentas instaladas…" : scannedAt ? `Verificado ${relativeTime(scannedAt)}` : "Ainda não verificado"}
        </span>
        <Button size="sm" variant="ghost" className="ml-auto" onClick={() => void scan()} disabled={scanning}>
          {scanning ? <Loader2 className="animate-spin" /> : <RefreshCw />} Verificar novamente
        </Button>
      </div>
      {tools.length === 0 && scanning ? (
        <div className="flex h-24 items-center justify-center text-subtle">
          <Loader2 className="size-4 animate-spin" />
        </div>
      ) : (
        <div className={cn("grid gap-3", !compact && "md:grid-cols-2")}>
          {groups
            .filter((g) => g.items.length)
            .map((g) => (
              <div key={g.c} className="overflow-hidden rounded-lg border border-border bg-bg-2">
                <div className="border-b border-border px-3 py-1.5 text-[10.5px] font-medium uppercase tracking-wider text-subtle">{CATEGORY_LABEL[g.c]}</div>
                <div className="divide-y divide-border">
                  {g.items.map((t) => (
                    <ToolRow key={t.id} t={t} />
                  ))}
                </div>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}
