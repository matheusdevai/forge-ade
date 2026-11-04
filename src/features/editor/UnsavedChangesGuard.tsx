import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button, Dialog, DialogContent } from "@/components/ui";
import { saveAllWithFeedback } from "./saveAll";
import { dirtyFiles } from "./status";

/**
 * Intercepts the window close when there are unsaved buffers and asks the
 * user to save, discard or cancel.
 */
export function UnsavedChangesGuard() {
  const [pending, setPending] = useState<string[] | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        const dirty = dirtyFiles();
        if (dirty.length === 0) return;
        event.preventDefault();
        setPending(dirty.map((d) => d.relPath));
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const quit = () => void getCurrentWindow().destroy();

  return (
    <Dialog open={!!pending} onOpenChange={(o) => !o && setPending(null)}>
      <DialogContent title="Há alterações não salvas" description="Escolha o que fazer antes de fechar o FORGE.">
        <div className="px-5 py-4 text-[13px] text-muted">
          <ul className="max-h-40 overflow-auto rounded-md border border-border bg-bg-2 p-2 font-mono text-[11.5px]">
            {pending?.map((p) => (
              <li key={p} className="truncate">
                {p}
              </li>
            ))}
          </ul>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setPending(null)}>
              Cancelar
            </Button>
            <Button variant="danger" onClick={quit}>
              Fechar sem salvar
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                if (await saveAllWithFeedback()) quit();
              }}
            >
              Salvar tudo e fechar
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
