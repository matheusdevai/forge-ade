import { Fragment, useMemo } from "react";
import { createPortal } from "react-dom";
import { Group, Panel, Separator } from "react-resizable-panels";
import { FolderTree, LayoutDashboard, X } from "lucide-react";
import { Button, EmptyState, Tooltip } from "@/components/ui";
import { usePaneDrag } from "./dnd";
import { addPane, findPane, restorePane, setSizes, type LayoutNode } from "./layout";
import { PaneFrame } from "./PaneFrame";
import { PANE_REGISTRY } from "./registry";
import { useLayout } from "./store";

function NodeView({ node, workspaceId }: { node: LayoutNode; workspaceId: string }) {
  const apply = useLayout((s) => s.apply);
  // Remount the group whenever its structure changes so default sizes apply.
  const structureKey = node.type === "split" ? `${node.orientation}:${node.children.map((c) => c.id).join(",")}` : node.id;
  const defaultLayout = useMemo(
    () => (node.type === "split" ? Object.fromEntries(node.children.map((c, i) => [c.id, node.sizes[i] ?? 0])) : undefined),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [structureKey],
  );

  if (node.type === "pane") return <PaneFrame pane={node} workspaceId={workspaceId} />;

  return (
    <Group
      key={structureKey}
      id={node.id}
      orientation={node.orientation}
      defaultLayout={defaultLayout}
      onLayoutChanged={(layout, meta) => {
        if (!meta.isUserInteraction) return;
        apply((doc) => setSizes(doc, node.id, node.children.map((c) => layout[c.id] ?? 0)));
      }}
      className="h-full w-full"
    >
      {node.children.map((child, i) => (
        <Fragment key={child.id}>
          {i > 0 && <Separator />}
          <Panel id={child.id} minSize="12">
            <NodeView node={child} workspaceId={workspaceId} />
          </Panel>
        </Fragment>
      ))}
    </Group>
  );
}

/** Tray with minimized panes. */
function MinimizedTray() {
  const minimized = useLayout((s) => s.doc?.minimized ?? []);
  const apply = useLayout((s) => s.apply);
  const closePaneById = useLayout((s) => s.closePaneById);
  if (minimized.length === 0) return null;
  return (
    <div className="flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-t border-border bg-bg px-2">
      <span className="mr-1 text-[10.5px] uppercase tracking-wider text-subtle">Minimizados</span>
      {minimized.map(({ pane }) => {
        const def = PANE_REGISTRY[pane.kind];
        return (
          <div key={pane.id} className="group flex h-6 shrink-0 items-center rounded-md border border-border bg-surface">
            <Tooltip content="Restaurar" side="top">
              <button type="button" className="flex h-full items-center gap-1.5 pr-1 pl-2 text-[12px] text-muted hover:text-fg" onClick={() => apply((d) => restorePane(d, pane.id))}>
                <def.icon className="size-3.5" /> {def.label}
              </button>
            </Tooltip>
            <button
              type="button"
              aria-label={`Fechar ${def.label}`}
              className="flex h-full items-center px-1 text-subtle opacity-0 group-hover:opacity-100 hover:text-fg"
              onClick={() => closePaneById(pane.id)}
            >
              <X className="size-3" />
            </button>
          </div>
        );
      })}
    </div>
  );
}

/** Floating label that follows the pointer while dragging a pane. */
function DragGhost() {
  const { source, x, y } = usePaneDrag();
  if (!source) return null;
  const def = PANE_REGISTRY[source.kind];
  return createPortal(
    <div
      className="pointer-events-none fixed z-[70] flex items-center gap-1.5 rounded-md border border-accent/50 bg-elevated px-2.5 py-1 text-xs text-fg shadow-xl shadow-black/50"
      style={{ left: x + 12, top: y + 12 }}
    >
      <def.icon className="size-3.5 text-accent" /> {def.label}
    </div>,
    document.body,
  );
}

export function PaneGrid({ workspaceId }: { workspaceId: string }) {
  const doc = useLayout((s) => s.doc);
  const apply = useLayout((s) => s.apply);
  const reset = useLayout((s) => s.reset);

  if (!doc) return <div className="h-full animate-pulse bg-bg-2" />;

  const maximized = doc.maximizedId ? findPane(doc, doc.maximizedId) : null;
  return (
    <div className="flex h-full w-full flex-col">
      <div className="min-h-0 flex-1">
        {!doc.root ? (
          <EmptyState
            className="h-full"
            icon={<LayoutDashboard />}
            title="Nenhum pane aberto"
            description="Adicione um pane, restaure um minimizado ou aplique um layout."
            action={
              <div className="flex gap-2">
                <Button onClick={() => apply((d) => addPane(d, "explorer"))}>
                  <FolderTree /> Files
                </Button>
                <Button variant="ghost" onClick={reset}>
                  Restaurar layout padrão
                </Button>
              </div>
            }
          />
        ) : maximized ? (
          <PaneFrame pane={maximized} workspaceId={workspaceId} />
        ) : (
          <NodeView node={doc.root} workspaceId={workspaceId} />
        )}
      </div>
      <MinimizedTray />
      <DragGhost />
    </div>
  );
}
