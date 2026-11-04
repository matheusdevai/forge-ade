/**
 * Pane layout model — a pure, serializable split tree.
 *
 * - A `pane` leaf is an independent session surface (explorer, terminal, agent…).
 * - A `split` arranges children side by side ("horizontal") or stacked ("vertical"),
 *   with `sizes` as percentages summing to 100.
 * - `minimized` holds panes parked in the tray, with an anchor so they can be
 *   restored where they were.
 *
 * All operations are immutable and return a new document (or the same one when
 * nothing changes), so they are easy to test, persist and later undo.
 */

export const PANE_KINDS = [
  "explorer",
  "overview",
  "search",
  "terminal",
  "editor",
  "git",
  "agent",
  "claude",
  "codex",
  "gemini",
  "browser",
  "preview",
  "logs",
  "monitor",
] as const;
export type PaneKind = (typeof PANE_KINDS)[number];
export type Orientation = "horizontal" | "vertical";
export type EdgeZone = "left" | "right" | "top" | "bottom";
export type DropZone = EdgeZone | "center";

export type PaneNode = { type: "pane"; id: string; kind: PaneKind; state?: Record<string, unknown> };
export type SplitNode = { type: "split"; id: string; orientation: Orientation; children: LayoutNode[]; sizes: number[] };
export type LayoutNode = PaneNode | SplitNode;
export type MinimizedPane = { pane: PaneNode; anchorId: string | null; zone: EdgeZone };

export type LayoutDoc = {
  version: 1;
  root: LayoutNode | null;
  focusedId: string | null;
  maximizedId: string | null;
  minimized: MinimizedPane[];
};

export const MAX_PANES = 16;

/**
 * State keys bound to live runtime resources (e.g. a PTY process). They are
 * dropped when a pane is copied, so a copy never hijacks the original's process.
 */
export const EPHEMERAL_STATE_KEYS = ["terminalId"];

export function copyState(state: Record<string, unknown> | undefined): Record<string, unknown> | undefined {
  if (!state) return undefined;
  const copy = structuredClone(state);
  for (const k of EPHEMERAL_STATE_KEYS) delete copy[k];
  return Object.keys(copy).length ? copy : undefined;
}

let counter = 0;
export function newId(prefix: string): string {
  counter = (counter + 1) % 1_000_000;
  return `${prefix}_${Date.now().toString(36)}${counter.toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

export function createPane(kind: PaneKind): PaneNode {
  return { type: "pane", id: newId("p"), kind };
}

export function split(orientation: Orientation, children: LayoutNode[], sizes?: number[]): SplitNode {
  return { type: "split", id: newId("s"), orientation, children, sizes: normalizeSizes(sizes ?? [], children.length) };
}

export function emptyLayout(): LayoutDoc {
  return { version: 1, root: null, focusedId: null, maximizedId: null, minimized: [] };
}

export function docFromRoot(root: LayoutNode | null): LayoutDoc {
  return { ...emptyLayout(), root, focusedId: panes(root)[0]?.id ?? null };
}

export function defaultLayout(): LayoutDoc {
  return docFromRoot(split("horizontal", [createPane("explorer"), createPane("editor"), createPane("overview")], [20, 56, 24]));
}

export function panes(node: LayoutNode | null): PaneNode[] {
  if (!node) return [];
  return node.type === "pane" ? [node] : node.children.flatMap(panes);
}

export function paneCount(doc: LayoutDoc): number {
  return panes(doc.root).length + doc.minimized.length;
}

export function findPane(doc: LayoutDoc, id: string): PaneNode | null {
  return panes(doc.root).find((p) => p.id === id) ?? null;
}

function evenSizes(n: number): number[] {
  const base = Math.floor((100 / n) * 100) / 100;
  const sizes = Array.from({ length: n }, () => base);
  sizes[n - 1] = Math.round((100 - base * (n - 1)) * 100) / 100;
  return sizes;
}

export function normalizeSizes(sizes: number[], n: number): number[] {
  if (sizes.length !== n || sizes.some((s) => !Number.isFinite(s) || s <= 0)) return evenSizes(n);
  const total = sizes.reduce((a, b) => a + b, 0);
  return sizes.map((s) => Math.round((s / total) * 10000) / 100);
}

/** Maps every node top-down; returning null removes it. Single-child splits collapse. */
function mapNode(node: LayoutNode, fn: (n: LayoutNode) => LayoutNode | null): LayoutNode | null {
  const mapped = fn(node);
  if (!mapped || mapped.type === "pane") return mapped;
  const kids: LayoutNode[] = [];
  const keptSizes: number[] = [];
  mapped.children.forEach((c, i) => {
    const m = mapNode(c, fn);
    if (m) {
      kids.push(m);
      keptSizes.push(mapped.sizes[i] ?? 0);
    }
  });
  if (kids.length === 0) return null;
  if (kids.length === 1) return kids[0]!;
  return { ...mapped, children: kids, sizes: normalizeSizes(keptSizes, kids.length) };
}

function removePaneNode(root: LayoutNode | null, id: string): LayoutNode | null {
  return root ? mapNode(root, (n) => (n.type === "pane" && n.id === id ? null : n)) : null;
}

function placement(zone: EdgeZone): { orientation: Orientation; before: boolean } {
  return {
    orientation: zone === "left" || zone === "right" ? "horizontal" : "vertical",
    before: zone === "left" || zone === "top",
  };
}

/**
 * Inserts `node` next to pane `targetId`. Joins the parent split when it has the
 * same orientation (halving the target's share), otherwise wraps the target in
 * a new split. Returns null when the target is not found.
 */
function insertRelative(root: LayoutNode, targetId: string, node: PaneNode, zone: EdgeZone): LayoutNode | null {
  const { orientation, before } = placement(zone);
  let done = false;
  const visit = (n: LayoutNode): LayoutNode => {
    if (done) return n;
    if (n.type === "split") {
      const idx = n.children.findIndex((c) => c.type === "pane" && c.id === targetId);
      if (idx >= 0 && n.orientation === orientation) {
        done = true;
        const children = [...n.children];
        children.splice(before ? idx : idx + 1, 0, node);
        const sizes = [...n.sizes];
        const half = (sizes[idx] ?? 50) / 2;
        sizes.splice(idx, 1, half, half);
        return { ...n, children, sizes: normalizeSizes(sizes, children.length) };
      }
      return { ...n, children: n.children.map(visit) };
    }
    if (n.id === targetId) {
      done = true;
      return split(orientation, before ? [node, n] : [n, node], [50, 50]);
    }
    return n;
  };
  const out = visit(root);
  return done ? out : null;
}

/** Appends a pane at the right edge of the layout (or as root when empty). */
function appendNode(doc: LayoutDoc, node: PaneNode): LayoutDoc {
  if (!doc.root) return { ...doc, root: node, focusedId: node.id, maximizedId: null };
  if (doc.root.type === "split" && doc.root.orientation === "horizontal") {
    const n = doc.root.children.length + 1;
    const scaled = doc.root.sizes.map((s) => (s * (n - 1)) / n);
    return {
      ...doc,
      root: { ...doc.root, children: [...doc.root.children, node], sizes: normalizeSizes([...scaled, 100 / n], n) },
      focusedId: node.id,
      maximizedId: null,
    };
  }
  return { ...doc, root: split("horizontal", [doc.root, node], [60, 40]), focusedId: node.id, maximizedId: null };
}

/** Splits `paneId`, placing a new pane of `kind` after it. */
export function splitPane(doc: LayoutDoc, paneId: string, orientation: Orientation, kind: PaneKind): LayoutDoc {
  if (paneCount(doc) >= MAX_PANES) return doc;
  const created = createPane(kind);
  if (!doc.root) return { ...doc, root: created, focusedId: created.id };
  const root = insertRelative(doc.root, paneId, created, orientation === "horizontal" ? "right" : "bottom");
  return root ? { ...doc, root, focusedId: created.id, maximizedId: null } : doc;
}

export function addPane(doc: LayoutDoc, kind: PaneKind): LayoutDoc {
  if (paneCount(doc) >= MAX_PANES) return doc;
  return appendNode(doc, createPane(kind));
}

export function closePane(doc: LayoutDoc, paneId: string): LayoutDoc {
  const minimized = doc.minimized.filter((m) => m.pane.id !== paneId);
  if (minimized.length !== doc.minimized.length) return { ...doc, minimized };
  if (!findPane(doc, paneId)) return doc;
  const root = removePaneNode(doc.root, paneId);
  const remaining = panes(root);
  return {
    ...doc,
    root,
    focusedId: doc.focusedId === paneId ? (remaining[0]?.id ?? null) : doc.focusedId,
    maximizedId: doc.maximizedId === paneId ? null : doc.maximizedId,
  };
}

/** Duplicates a pane (same kind and state) next to the original. */
export function duplicatePane(doc: LayoutDoc, paneId: string): LayoutDoc {
  const src = findPane(doc, paneId);
  if (!src) return doc;
  const next = splitPane(doc, paneId, "horizontal", src.kind);
  const state = copyState(src.state);
  if (next === doc || !next.focusedId || !state) return next;
  return setPaneState(next, next.focusedId, state);
}

/**
 * Moves pane `srcId` relative to pane `targetId`. Edge zones dock it on that
 * side; `center` swaps the two panes.
 */
export function movePane(doc: LayoutDoc, srcId: string, targetId: string, zone: DropZone): LayoutDoc {
  if (srcId === targetId || !doc.root) return doc;
  const src = findPane(doc, srcId);
  const target = findPane(doc, targetId);
  if (!src || !target) return doc;

  if (zone === "center") {
    const root = mapNode(doc.root, (n) => (n.type !== "pane" ? n : n.id === srcId ? target : n.id === targetId ? src : n));
    return { ...doc, root, focusedId: srcId, maximizedId: null };
  }
  const without = removePaneNode(doc.root, srcId);
  if (!without) return doc;
  const root = insertRelative(without, targetId, src, zone);
  return root ? { ...doc, root, focusedId: srcId, maximizedId: null } : doc;
}

function findParent(node: LayoutNode, id: string): { parent: SplitNode; index: number } | null {
  if (node.type === "pane") return null;
  const index = node.children.findIndex((c) => c.id === id);
  if (index >= 0) return { parent: node, index };
  for (const c of node.children) {
    const r = findParent(c, id);
    if (r) return r;
  }
  return null;
}

/** Parks a pane in the tray, remembering a neighbour to restore it next to. */
export function minimizePane(doc: LayoutDoc, paneId: string): LayoutDoc {
  const pane = findPane(doc, paneId);
  if (!pane || !doc.root) return doc;
  let anchorId: string | null = null;
  let zone: EdgeZone = "right";
  const found = findParent(doc.root, paneId);
  if (found) {
    const { parent, index } = found;
    const before = index + 1 < parent.children.length; // pane sits before its sibling
    const sibling = parent.children[before ? index + 1 : index - 1]!;
    const siblingPanes = panes(sibling);
    anchorId = (before ? siblingPanes[0] : siblingPanes[siblingPanes.length - 1])?.id ?? null;
    zone = parent.orientation === "horizontal" ? (before ? "left" : "right") : before ? "top" : "bottom";
  }
  const root = removePaneNode(doc.root, paneId);
  return {
    ...doc,
    root,
    minimized: [...doc.minimized, { pane, anchorId, zone }],
    focusedId: doc.focusedId === paneId ? (panes(root)[0]?.id ?? null) : doc.focusedId,
    maximizedId: doc.maximizedId === paneId ? null : doc.maximizedId,
  };
}

/** Restores a minimized pane next to its anchor (or at the right edge if the anchor is gone). */
export function restorePane(doc: LayoutDoc, paneId: string): LayoutDoc {
  const entry = doc.minimized.find((m) => m.pane.id === paneId);
  if (!entry) return doc;
  const base = { ...doc, minimized: doc.minimized.filter((m) => m.pane.id !== paneId) };
  if (base.root && entry.anchorId && findPane(base, entry.anchorId)) {
    const root = insertRelative(base.root, entry.anchorId, entry.pane, entry.zone);
    if (root) return { ...base, root, focusedId: paneId, maximizedId: null };
  }
  return appendNode(base, entry.pane);
}

export function setPaneKind(doc: LayoutDoc, paneId: string, kind: PaneKind): LayoutDoc {
  if (!doc.root) return doc;
  return { ...doc, root: mapNode(doc.root, (n) => (n.type === "pane" && n.id === paneId ? { type: "pane", id: n.id, kind } : n)) };
}

export function setPaneState(doc: LayoutDoc, paneId: string, state: Record<string, unknown>): LayoutDoc {
  if (!doc.root) return doc;
  return { ...doc, root: mapNode(doc.root, (n) => (n.type === "pane" && n.id === paneId ? { ...n, state } : n)) };
}

export function setSizes(doc: LayoutDoc, splitId: string, sizes: number[]): LayoutDoc {
  if (!doc.root) return doc;
  return {
    ...doc,
    root: mapNode(doc.root, (n) => (n.type === "split" && n.id === splitId ? { ...n, sizes: normalizeSizes(sizes, n.children.length) } : n)),
  };
}

export function focusPane(doc: LayoutDoc, paneId: string): LayoutDoc {
  return findPane(doc, paneId) && doc.focusedId !== paneId ? { ...doc, focusedId: paneId } : doc;
}

export function toggleMaximize(doc: LayoutDoc, paneId: string): LayoutDoc {
  if (!findPane(doc, paneId)) return doc;
  return { ...doc, maximizedId: doc.maximizedId === paneId ? null : paneId, focusedId: paneId };
}

/** Moves focus to the next/previous pane in reading order. */
export function cycleFocus(doc: LayoutDoc, delta: 1 | -1): LayoutDoc {
  const list = panes(doc.root);
  if (list.length === 0) return doc;
  const idx = Math.max(0, list.findIndex((p) => p.id === doc.focusedId));
  const next = list[(idx + delta + list.length) % list.length]!;
  return { ...doc, focusedId: next.id, maximizedId: doc.maximizedId ? next.id : null };
}

/** Drop zone for a pointer at relative position (rx, ry) ∈ [0,1]² inside a pane. */
export function dropZoneFor(rx: number, ry: number, edge = 0.25): DropZone {
  const x = Math.min(1, Math.max(0, rx));
  const y = Math.min(1, Math.max(0, ry));
  const d: [EdgeZone, number][] = [
    ["left", x],
    ["right", 1 - x],
    ["top", y],
    ["bottom", 1 - y],
  ];
  const [zone, dist] = d.reduce((a, b) => (b[1] < a[1] ? b : a));
  return dist < edge ? zone : "center";
}

// ── Templates (saved layouts) ─────────────────────────────────────────────

/** Strips pane state and the tray: a reusable layout shape. */
export function toTemplate(doc: LayoutDoc): LayoutDoc {
  const strip = (n: LayoutNode): LayoutNode =>
    n.type === "pane" ? { type: "pane", id: n.id, kind: n.kind } : { ...n, children: n.children.map(strip) };
  return { version: 1, root: doc.root ? strip(doc.root) : null, focusedId: doc.focusedId, maximizedId: null, minimized: [] };
}

/** Deep copy with fresh ids (so several sessions never share pane ids). */
export function instantiate(doc: LayoutDoc, keepState = false): LayoutDoc {
  const ids = new Map<string, string>();
  const remap = (id: string, prefix: string) => {
    const fresh = newId(prefix);
    ids.set(id, fresh);
    return fresh;
  };
  const clonePane = (p: PaneNode): PaneNode => {
    const state = keepState ? copyState(p.state) : undefined;
    return { type: "pane", id: remap(p.id, "p"), kind: p.kind, ...(state ? { state } : {}) };
  };
  const clone = (n: LayoutNode): LayoutNode =>
    n.type === "pane" ? clonePane(n) : { ...n, id: remap(n.id, "s"), children: n.children.map(clone), sizes: [...n.sizes] };
  const root = doc.root ? clone(doc.root) : null;
  const minimized = keepState ? doc.minimized.map((m) => ({ ...m, pane: clonePane(m.pane) })) : [];
  const minimizedFixed = minimized.map((m) => ({ ...m, anchorId: m.anchorId ? (ids.get(m.anchorId) ?? null) : null }));
  return {
    version: 1,
    root,
    focusedId: (doc.focusedId && ids.get(doc.focusedId)) || (panes(root)[0]?.id ?? null),
    maximizedId: null,
    minimized: minimizedFixed,
  };
}

// ── Persistence ───────────────────────────────────────────────────────────

function validPane(n: unknown, seen: Set<string>): n is PaneNode {
  if (!n || typeof n !== "object") return false;
  const p = n as Record<string, unknown>;
  if (p.type !== "pane" || typeof p.id !== "string" || seen.has(p.id)) return false;
  if (p.state !== undefined && (typeof p.state !== "object" || p.state === null || Array.isArray(p.state))) return false;
  seen.add(p.id);
  return PANE_KINDS.includes(p.kind as PaneKind);
}

function validNode(n: unknown, seen: Set<string>, depth: number): n is LayoutNode {
  if (!n || typeof n !== "object" || depth > 12) return false;
  const node = n as Record<string, unknown>;
  if (node.type === "pane") return validPane(n, seen);
  if (node.type !== "split" || typeof node.id !== "string" || seen.has(node.id)) return false;
  seen.add(node.id);
  return (
    (node.orientation === "horizontal" || node.orientation === "vertical") &&
    Array.isArray(node.children) &&
    node.children.length >= 2 &&
    Array.isArray(node.sizes) &&
    node.children.every((c) => validNode(c, seen, depth + 1))
  );
}

const EDGE_ZONES: EdgeZone[] = ["left", "right", "top", "bottom"];

/** Parses a persisted layout; returns `null` when it is missing or invalid. */
export function parseLayout(json: string | null): LayoutDoc | null {
  if (!json) return null;
  try {
    const raw = JSON.parse(json) as Partial<LayoutDoc>;
    if (!raw || raw.version !== 1) return null;
    const seen = new Set<string>();
    if (raw.root != null && !validNode(raw.root, seen, 0)) return null;
    const minimizedRaw: unknown[] = Array.isArray(raw.minimized) ? raw.minimized : [];
    const minimized: MinimizedPane[] = [];
    for (const m of minimizedRaw) {
      const e = m as Partial<MinimizedPane>;
      if (!e || !validPane(e.pane, seen)) return null;
      minimized.push({
        pane: e.pane,
        anchorId: typeof e.anchorId === "string" ? e.anchorId : null,
        zone: EDGE_ZONES.includes(e.zone as EdgeZone) ? (e.zone as EdgeZone) : "right",
      });
    }
    let root: LayoutNode | null = raw.root ?? null;
    // Re-normalize sizes defensively.
    if (root) root = mapNode(root, (n) => (n.type === "split" ? { ...n, sizes: normalizeSizes(n.sizes, n.children.length) } : n));
    const ids = new Set(panes(root).map((p) => p.id));
    return {
      version: 1,
      root,
      focusedId: raw.focusedId && ids.has(raw.focusedId) ? raw.focusedId : (panes(root)[0]?.id ?? null),
      maximizedId: raw.maximizedId && ids.has(raw.maximizedId) ? raw.maximizedId : null,
      minimized,
    };
  } catch {
    return null;
  }
}

export function serializeLayout(doc: LayoutDoc): string {
  return JSON.stringify(doc);
}
