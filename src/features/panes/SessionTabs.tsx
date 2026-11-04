import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Copy, LayoutTemplate, MoreHorizontal, Pencil, Plus, Save, Settings2, X } from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Dialog,
  DialogContent,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Input,
  Label,
  Tooltip,
} from "@/components/ui";
import { modKey } from "@/lib/utils";
import { useApp } from "@/stores/app";
import { parseLayout, type LayoutDoc } from "./layout";
import { BUILTIN_PRESETS } from "./presets";
import { useLayout, usePresets, type SessionMeta } from "./store";

/** Replaces the active session's layout, offering an undo. */
export function applyLayoutWithUndo(template: LayoutDoc, name: string) {
  const { doc, applyTemplate, apply } = useLayout.getState();
  if (!doc) return;
  const previous = doc;
  applyTemplate(template);
  toast.success(`Layout “${name}” aplicado`, {
    action: { label: "Desfazer", onClick: () => apply(() => previous) },
  });
}

function TabName({ session, editing, onDone }: { session: SessionMeta; editing: boolean; onDone: () => void }) {
  const rename = useLayout((s) => s.renameSession);
  const [value, setValue] = useState(session.name);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editing) {
      setValue(session.name);
      requestAnimationFrame(() => ref.current?.select());
    }
  }, [editing, session.name]);
  if (!editing) return <span className="truncate">{session.name}</span>;
  const commit = () => {
    const v = value.trim();
    if (v && v !== session.name) void rename(session.id, v);
    onDone();
  };
  return (
    <input
      ref={ref}
      value={value}
      maxLength={60}
      aria-label="Nome da sessão"
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") onDone();
        e.stopPropagation();
      }}
      onClick={(e) => e.stopPropagation()}
      className="w-28 rounded-sm border border-accent/60 bg-bg-2 px-1 text-[12px] text-fg outline-none"
    />
  );
}

function SessionTab({ session, index, count }: { session: SessionMeta; index: number; count: number }) {
  const active = useLayout((s) => s.activeId === session.id);
  const { switchSession, closeSession, duplicateSession, moveSession } = useLayout.getState();
  const [editing, setEditing] = useState(false);

  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={0}
      onClick={() => switchSession(session.id)}
      onDoubleClick={() => setEditing(true)}
      onKeyDown={(e) => e.key === "Enter" && !editing && switchSession(session.id)}
      className={
        "group relative flex h-8 max-w-[200px] min-w-[96px] shrink-0 cursor-default items-center gap-1 border-r border-border pr-1 pl-3 text-[12px] " +
        (active ? "bg-bg-2 text-fg" : "text-muted hover:bg-surface hover:text-fg")
      }
      title={editing ? undefined : `${session.name} — duplo clique para renomear`}
    >
      {active && <span className="absolute inset-x-0 top-0 h-px bg-accent" />}
      <span className="min-w-0 flex-1">
        <TabName session={session} editing={editing} onDone={() => setEditing(false)} />
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={`Ações da sessão ${session.name}`}
            onClick={(e) => e.stopPropagation()}
            className="flex size-5 items-center justify-center rounded text-subtle opacity-0 group-hover:opacity-100 hover:bg-elevated hover:text-fg data-[state=open]:opacity-100"
          >
            <MoreHorizontal className="size-3.5" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onSelect={() => setEditing(true)}>
            <Pencil /> Renomear
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void duplicateSession(session.id)}>
            <Copy /> Duplicar sessão
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem disabled={index === 0} onSelect={() => void moveSession(session.id, -1)}>
            <ArrowLeft /> Mover para a esquerda
          </DropdownMenuItem>
          <DropdownMenuItem disabled={index === count - 1} onSelect={() => void moveSession(session.id, 1)}>
            <ArrowRight /> Mover para a direita
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem disabled={count <= 1} className="text-error" onSelect={() => void closeSession(session.id)}>
            <X /> Fechar sessão
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {count > 1 && (
        <button
          type="button"
          aria-label={`Fechar sessão ${session.name}`}
          onClick={(e) => {
            e.stopPropagation();
            void closeSession(session.id);
          }}
          className="flex size-5 items-center justify-center rounded text-subtle opacity-0 group-hover:opacity-100 hover:bg-elevated hover:text-fg"
        >
          <X className="size-3" />
        </button>
      )}
    </div>
  );
}

function SavePresetDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const saveCurrent = usePresets((s) => s.saveCurrent);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    if (open) setName("");
  }, [open]);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    setSaving(true);
    const ok = await saveCurrent(name.trim());
    setSaving(false);
    if (ok) {
      toast.success("Layout salvo", { description: name.trim() });
      onOpenChange(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Salvar layout atual" description="Salva a disposição e os tipos dos panes (não o conteúdo) para reutilizar em qualquer workspace.">
        <form onSubmit={submit} className="space-y-4 px-5 py-4">
          <div>
            <Label htmlFor="preset-name">Nome</Label>
            <Input id="preset-name" autoFocus maxLength={60} value={name} onChange={(e) => setName(e.target.value)} placeholder="Ex.: Revisão de código" />
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancelar
            </Button>
            <Button type="submit" variant="primary" disabled={!name.trim() || saving}>
              Salvar
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function LayoutsMenu() {
  const presets = usePresets((s) => s.items);
  const loaded = usePresets((s) => s.loaded);
  const refresh = usePresets((s) => s.refresh);
  const navigate = useApp((s) => s.navigate);
  const [saveOpen, setSaveOpen] = useState(false);

  useEffect(() => {
    if (!loaded) void refresh().catch(() => undefined);
  }, [loaded, refresh]);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button size="sm" variant="ghost" className="h-6">
            <LayoutTemplate /> Layouts
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-[240px]">
          <DropdownMenuLabel>Aplicar à sessão atual</DropdownMenuLabel>
          {BUILTIN_PRESETS.map((p) => (
            <DropdownMenuItem key={p.id} onSelect={() => applyLayoutWithUndo(p.build(), p.name)}>
              <LayoutTemplate /> {p.name}
              <span className="ml-auto text-[11px] text-subtle">{p.description}</span>
            </DropdownMenuItem>
          ))}
          {presets.length > 0 && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>Meus layouts</DropdownMenuLabel>
              {presets.map((p) => (
                <DropdownMenuItem
                  key={p.id}
                  onSelect={() => {
                    const doc = parseLayout(p.layoutJson);
                    if (doc) applyLayoutWithUndo(doc, p.name);
                    else toast.error("Layout salvo inválido", { description: p.name });
                  }}
                >
                  <LayoutTemplate /> {p.name}
                </DropdownMenuItem>
              ))}
            </>
          )}
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setSaveOpen(true)}>
            <Save /> Salvar layout atual…
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => navigate({ view: "settings", section: "layouts" })}>
            <Settings2 /> Gerenciar layouts
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <SavePresetDialog open={saveOpen} onOpenChange={setSaveOpen} />
    </>
  );
}

export function SessionTabs() {
  const sessions = useLayout((s) => s.sessions);
  const newSession = useLayout((s) => s.newSession);
  return (
    <div className="flex h-8 shrink-0 items-stretch border-b border-border bg-bg" role="tablist" aria-label="Sessões do workspace">
      <div className="flex min-w-0 items-stretch overflow-x-auto">
        {sessions.map((s, i) => (
          <SessionTab key={s.id} session={s} index={i} count={sessions.length} />
        ))}
      </div>
      <Tooltip content={`Nova sessão (${modKey}T)`}>
        <button type="button" aria-label="Nova sessão" onClick={() => void newSession()} className="flex w-8 shrink-0 items-center justify-center text-subtle hover:bg-surface hover:text-fg">
          <Plus className="size-3.5" />
        </button>
      </Tooltip>
      <div className="ml-auto flex items-center pr-2">
        <LayoutsMenu />
      </div>
    </div>
  );
}
