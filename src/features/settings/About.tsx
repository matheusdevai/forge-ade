import { useCallback, useEffect, useState } from "react";
import { Download, FileWarning, FolderOpen, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, Dialog, DialogContent } from "@/components/ui";
import { api, errorMessage, type AppInfo, type CrashReport, type UpdateInfo } from "@/lib/ipc";
import { relativeTime } from "@/lib/utils";

function Updates({ info }: { info: AppInfo | null }) {
  const [state, setState] = useState<{ checking: boolean; result: UpdateInfo | null; error: string | null }>({ checking: false, result: null, error: null });
  const [installing, setInstalling] = useState(false);
  const check = async () => {
    setState({ checking: true, result: null, error: null });
    try {
      setState({ checking: false, result: await api.appCheckUpdate(), error: null });
    } catch (e) {
      setState({ checking: false, result: null, error: errorMessage(e) });
    }
  };
  const install = async () => {
    setInstalling(true);
    try {
      await api.appInstallUpdate();
    } catch (e) {
      setInstalling(false);
      toast.error("Falha ao instalar a atualização", { description: errorMessage(e) });
    }
  };
  const r = state.result;
  return (
    <Card className="px-4 py-3">
      <div className="flex items-center gap-2">
        <span className="text-[13px] font-medium">Atualizações</span>
        <span className="font-mono text-[11.5px] text-muted">v{info?.version ?? "…"}</span>
        <Button size="sm" variant="ghost" className="ml-auto" disabled={state.checking || installing} onClick={() => void check()}>
          {state.checking ? <Loader2 className="animate-spin" /> : <RefreshCw />} Verificar atualizações
        </Button>
      </div>
      <p className="mt-1 text-[11.5px] text-muted">
        Consulta as releases públicas de github.com/matheusdevai/forge-ade. Pacotes só são instalados com assinatura válida da chave do projeto.
      </p>
      {state.error && <p className="mt-2 text-[12px] text-error">{state.error}</p>}
      {r && !r.available && <p className="mt-2 text-[12px] text-success">Você está na versão mais recente ({r.current}).</p>}
      {r?.available && (
        <div className="mt-2 space-y-2 rounded-md border border-accent/30 bg-accent/5 p-3">
          <p className="text-[12.5px]">
            Nova versão <b>{r.version}</b> disponível{r.date ? ` · ${r.date.slice(0, 10)}` : ""}.
          </p>
          {r.notes && <pre className="max-h-40 overflow-auto font-sans text-[12px] whitespace-pre-wrap text-muted">{r.notes}</pre>}
          <Button size="sm" variant="primary" disabled={installing} onClick={() => void install()}>
            {installing ? <Loader2 className="animate-spin" /> : <Download />} Baixar, instalar e reiniciar
          </Button>
        </div>
      )}
    </Card>
  );
}

function Crashes() {
  const [items, setItems] = useState<CrashReport[] | null>(null);
  const [open, setOpen] = useState<{ file: string; text: string } | null>(null);
  const load = useCallback(() => void api.crashList().then(setItems).catch(() => setItems([])), []);
  useEffect(load, [load]);
  return (
    <Card className="px-4 py-3">
      <div className="flex items-center gap-2">
        <FileWarning className="size-4 text-muted" />
        <span className="text-[13px] font-medium">Relatórios de falha</span>
        {items && items.length > 0 && <Badge tone="warning">{items.length}</Badge>}
        <div className="ml-auto flex gap-1">
          <Button size="sm" variant="ghost" onClick={() => void api.crashReveal().catch(() => undefined)}>
            <FolderOpen /> Pasta
          </Button>
          {items && items.length > 0 && (
            <Button size="sm" variant="ghost" onClick={() => void api.crashClear().then(load)}>
              <Trash2 /> Limpar
            </Button>
          )}
        </div>
      </div>
      <p className="mt-1 text-[11.5px] text-muted">Ficam só neste Mac, com segredos mascarados. Nada é enviado — compartilhe um relatório se quiser ajuda.</p>
      {items && items.length === 0 && <p className="mt-2 text-[12px] text-success">Nenhuma falha registrada.</p>}
      {items?.slice(0, 10).map((c) => (
        <button
          key={c.file}
          className="mt-1 flex w-full items-center gap-2 rounded px-1 py-1 text-left text-[12px] hover:bg-elevated"
          onClick={() => void api.crashRead(c.file).then((text) => setOpen({ file: c.file, text }))}
        >
          <Badge>{c.source === "ui" ? "interface" : "núcleo"}</Badge>
          <span className="min-w-0 flex-1 truncate">{c.summary}</span>
          <span className="text-[11px] text-subtle">{relativeTime(c.createdAt)}</span>
        </button>
      ))}
      <Dialog open={!!open} onOpenChange={(o) => !o && setOpen(null)}>
        {open && (
          <DialogContent title="Relatório de falha" description={open.file} className="w-[min(760px,calc(100vw-32px))]">
            <pre className="max-h-[60vh] overflow-auto px-5 py-4 font-mono text-[11px] whitespace-pre-wrap text-muted">{open.text}</pre>
            <div className="flex justify-end border-t border-border px-5 py-3">
              <Button size="sm" variant="ghost" onClick={() => void navigator.clipboard.writeText(open.text).then(() => toast.success("Copiado"))}>
                Copiar
              </Button>
            </div>
          </DialogContent>
        )}
      </Dialog>
    </Card>
  );
}

export function AboutSection({ info }: { info: AppInfo | null }) {
  return (
    <div className="space-y-3 pt-3">
      <Card className="px-4 py-3">
        <p className="text-[13px] font-medium">FORGE — Build beyond limits.</p>
        <p className="mt-0.5 text-[12px] text-muted">Agentic Development Environment · v{info?.version ?? "…"} · {info ? `${info.platform} · ${info.arch}` : "…"}</p>
      </Card>
      <Updates info={info} />
      <Crashes />
    </div>
  );
}
