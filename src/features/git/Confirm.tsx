import { Button, Dialog, DialogContent } from "@/components/ui";

export type ConfirmRequest = {
  title: string;
  description?: string;
  body?: React.ReactNode;
  confirmLabel: string;
  danger?: boolean;
  run: () => unknown;
};

export function ConfirmDialog({ request, onClose }: { request: ConfirmRequest | null; onClose: () => void }) {
  return (
    <Dialog open={!!request} onOpenChange={(o) => !o && onClose()}>
      {request && (
        <DialogContent title={request.title} description={request.description}>
          <div className="px-5 py-4 text-[13px] text-muted">
            {request.body}
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={onClose}>
                Cancelar
              </Button>
              <Button
                variant={request.danger ? "danger" : "primary"}
                onClick={() => {
                  onClose();
                  void request.run();
                }}
              >
                {request.confirmLabel}
              </Button>
            </div>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
