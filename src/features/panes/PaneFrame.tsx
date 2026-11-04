import { Suspense } from "react";
import { Columns2, Copy, GripVertical, Maximize2, Minimize2, Minus, MoreHorizontal, Replace, Rows2, X } from "lucide-react";
import {
  Badge,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  EmptyState,
  Tooltip,
} from "@/components/ui";
import { cn } from "@/lib/utils";
import { beginPaneDrag, usePaneDrag } from "./dnd";
import {
  duplicatePane,
  focusPane,
  minimizePane,
  setPaneKind,
  splitPane,
  toggleMaximize,
  type DropZone,
  type Orientation,
  type PaneKind,
  type PaneNode,
} from "./layout";
import { PANE_GROUPS, PANE_REGISTRY } from "./registry";
import { useLayout } from "./store";

export function PaneKindMenuItems({ onPick, current }: { onPick: (kind: PaneKind) => void; current?: PaneKind }) {
  return (
    <>
      {PANE_GROUPS.map((g, gi) => (
        <div key={g.id}>
          {gi > 0 && <DropdownMenuSeparator />}
          <DropdownMenuLabel>{g.label}</DropdownMenuLabel>
          {Object.values(PANE_REGISTRY)
            .filter((d) => d.group === g.id)
            .map((d) => (
              <DropdownMenuItem key={d.kind} disabled={!d.component || d.kind === current} onSelect={() => onPick(d.kind)}>
                <d.icon /> {d.label}
                {!d.component && (
                  <Badge className="ml-auto" title={`Disponível no milestone ${d.milestone}`}>
                    {d.milestone}
                  </Badge>
                )}
              </DropdownMenuItem>
            ))}
        </div>
      ))}
    </>
  );
}

function SplitMenu({ pane, orientation }: { pane: PaneNode; orientation: Orientation }) {
  const apply = useLayout((s) => s.apply);
  const Icon = orientation === "horizontal" ? Columns2 : Rows2;
  const label = orientation === "horizontal" ? "Dividir à direita" : "Dividir abaixo";
  return (
    <DropdownMenu>
      <Tooltip content={label}>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" aria-label={label}>
            <Icon />
          </Button>
        </DropdownMenuTrigger>
      </Tooltip>
      <DropdownMenuContent align="end">
        <PaneKindMenuItems onPick={(kind) => apply((d) => splitPane(d, pane.id, orientation, kind))} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const ZONE_RECT: Record<DropZone, string> = {
  left: "inset-y-0 left-0 w-1/2",
  right: "inset-y-0 right-0 w-1/2",
  top: "inset-x-0 top-0 h-1/2",
  bottom: "inset-x-0 bottom-0 h-1/2",
  center: "inset-0",
};
const ZONE_LABEL: Record<DropZone, string> = {
  left: "Encaixar à esquerda",
  right: "Encaixar à direita",
  top: "Encaixar acima",
  bottom: "Encaixar abaixo",
  center: "Trocar de lugar",
};

function DropPreview({ paneId }: { paneId: string }) {
  const zone = usePaneDrag((s) => (s.target?.paneId === paneId ? s.target.zone : null));
  const dragging = usePaneDrag((s) => !!s.source);
  if (!dragging) return null;
  return (
    <div className="pointer-events-none absolute inset-0 z-20">
      {zone && (
        <div className={cn("absolute m-1 flex items-center justify-center rounded-md border-2 border-accent/80 bg-accent/15 transition-all", ZONE_RECT[zone])}>
          <span className="rounded bg-bg/90 px-2 py-0.5 text-[11px] font-medium text-accent">{ZONE_LABEL[zone]}</span>
        </div>
      )}
    </div>
  );
}

export function PaneFrame({ pane, workspaceId }: { pane: PaneNode; workspaceId: string }) {
  const apply = useLayout((s) => s.apply);
  const closePaneById = useLayout((s) => s.closePaneById);
  const focused = useLayout((s) => s.doc?.focusedId === pane.id);
  const maximized = useLayout((s) => s.doc?.maximizedId === pane.id);
  const isSource = usePaneDrag((s) => s.source?.paneId === pane.id);
  const def = PANE_REGISTRY[pane.kind];
  const Body = def.component;

  return (
    <section
      data-pane-id={pane.id}
      className={cn("relative flex h-full min-h-0 w-full min-w-0 flex-col bg-bg-2", isSource && "opacity-40")}
      onMouseDownCapture={() => apply((d) => focusPane(d, pane.id))}
      aria-label={`Pane ${def.label}`}
    >
      <header
        className={cn("flex h-8 shrink-0 items-center gap-1 border-b border-border pr-1", focused ? "bg-surface" : "bg-bg-2")}
        onDoubleClick={() => apply((d) => toggleMaximize(d, pane.id))}
      >
        <div
          className={cn("flex h-full min-w-0 flex-1 items-center gap-1.5 pl-1", !maximized && "cursor-grab active:cursor-grabbing")}
          onPointerDown={(e) => beginPaneDrag(e, pane.id, pane.kind)}
          title={maximized ? undefined : "Arraste para mover o pane"}
        >
          <GripVertical className={cn("size-3.5 shrink-0 text-subtle/60", maximized && "invisible")} />
          <def.icon className={cn("size-3.5 shrink-0", focused ? "text-accent" : "text-subtle")} />
          <span className={cn("truncate text-xs font-medium", focused ? "text-fg" : "text-muted")}>{def.label}</span>
        </div>
        <div className="flex items-center">
          <SplitMenu pane={pane} orientation="horizontal" />
          <SplitMenu pane={pane} orientation="vertical" />
          <Tooltip content="Minimizar">
            <Button variant="ghost" size="icon" aria-label="Minimizar" onClick={() => apply((d) => minimizePane(d, pane.id))}>
              <Minus />
            </Button>
          </Tooltip>
          <Tooltip content={maximized ? "Restaurar" : "Maximizar"}>
            <Button variant="ghost" size="icon" aria-label={maximized ? "Restaurar" : "Maximizar"} onClick={() => apply((d) => toggleMaximize(d, pane.id))}>
              {maximized ? <Minimize2 /> : <Maximize2 />}
            </Button>
          </Tooltip>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon" aria-label="Mais ações">
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => apply((d) => duplicatePane(d, pane.id))}>
                <Copy /> Duplicar pane
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>
                <span className="inline-flex items-center gap-1">
                  <Replace className="size-3" /> Trocar tipo
                </span>
              </DropdownMenuLabel>
              <PaneKindMenuItems current={pane.kind} onPick={(kind) => apply((d) => setPaneKind(d, pane.id, kind))} />
            </DropdownMenuContent>
          </DropdownMenu>
          <Tooltip content="Fechar pane">
            <Button variant="ghost" size="icon" aria-label="Fechar pane" onClick={() => closePaneById(pane.id)}>
              <X />
            </Button>
          </Tooltip>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-hidden">
        {Body ? (
          <Suspense fallback={<div className="h-full bg-bg-2" />}>
            <Body pane={pane} workspaceId={workspaceId} />
          </Suspense>
        ) : (
          <EmptyState className="h-full" title={`${def.label} ainda não disponível`} description={`Este tipo de pane chega no milestone ${def.milestone}.`} />
        )}
      </div>
      {focused && <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-accent/70" />}
      <DropPreview paneId={pane.id} />
    </section>
  );
}
