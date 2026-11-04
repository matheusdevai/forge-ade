import { describe, expect, it } from "vitest";
import {
  MAX_PANES,
  addPane,
  closePane,
  duplicatePane as duplicatePaneOf,
  createPane,
  docFromRoot,
  dropZoneFor,
  instantiate,
  minimizePane,
  movePane,
  paneCount,
  panes,
  parseLayout,
  restorePane,
  serializeLayout,
  setPaneState,
  split,
  splitPane,
  toTemplate,
  type LayoutDoc,
  type LayoutNode,
  type SplitNode,
} from "./layout";
import { BUILTIN_PRESETS } from "./presets";

/** Compact structural signature: panes by kind+index, splits as h(...)/v(...). */
function sig(n: LayoutNode | null, names: Map<string, string>): string {
  if (!n) return "∅";
  if (n.type === "pane") return names.get(n.id) ?? "?";
  return `${n.orientation[0]}(${n.children.map((c) => sig(c, names)).join(",")})`;
}

function three(): { doc: LayoutDoc; a: string; b: string; c: string; s: (d: LayoutDoc) => string } {
  const [A, B, C] = [createPane("explorer"), createPane("overview"), createPane("explorer")];
  const doc = docFromRoot(split("horizontal", [A, B, C]));
  const names = new Map([
    [A.id, "A"],
    [B.id, "B"],
    [C.id, "C"],
  ]);
  return { doc, a: A.id, b: B.id, c: C.id, s: (d) => sig(d.root, names) };
}

describe("movePane", () => {
  it("docks on each edge", () => {
    const { doc, a, c, s } = three();
    expect(s(movePane(doc, a, c, "right"))).toBe("h(B,C,A)");
    expect(s(movePane(doc, c, a, "left"))).toBe("h(C,A,B)");
    expect(s(movePane(doc, a, c, "bottom"))).toBe("h(B,v(C,A))");
    expect(s(movePane(doc, a, c, "top"))).toBe("h(B,v(A,C))");
  });

  it("swaps on center and keeps sizes", () => {
    const { doc, a, c, s } = three();
    const next = movePane(doc, a, c, "center");
    expect(s(next)).toBe("h(C,B,A)");
    expect((next.root as SplitNode).sizes).toEqual((doc.root as SplitNode).sizes);
    expect(next.focusedId).toBe(a);
  });

  it("is a no-op for self or unknown panes", () => {
    const { doc, a } = three();
    expect(movePane(doc, a, a, "left")).toBe(doc);
    expect(movePane(doc, a, "ghost", "left")).toBe(doc);
  });

  it("collapses the source's former parent", () => {
    const { doc, a, b, c, s } = three();
    const nested = movePane(doc, a, c, "bottom"); // h(B, v(C, A))
    const back = movePane(nested, a, b, "left"); // v collapses → h(A, B, C)
    expect(s(back)).toBe("h(A,B,C)");
    expect(panes(back.root)).toHaveLength(3);
    expect(Math.round((back.root as SplitNode).sizes.reduce((x, y) => x + y))).toBe(100);
  });
});

describe("dropZoneFor", () => {
  it("maps edges and center", () => {
    expect(dropZoneFor(0.05, 0.5)).toBe("left");
    expect(dropZoneFor(0.95, 0.5)).toBe("right");
    expect(dropZoneFor(0.5, 0.05)).toBe("top");
    expect(dropZoneFor(0.5, 0.95)).toBe("bottom");
    expect(dropZoneFor(0.5, 0.5)).toBe("center");
    expect(dropZoneFor(-3, 0.5)).toBe("left");
  });
});

describe("minimize / restore", () => {
  it("restores next to its original neighbour", () => {
    const { doc, a, b, s } = three();
    const min = minimizePane(doc, b);
    expect(s(min)).toBe("h(A,C)");
    expect(min.minimized).toHaveLength(1);
    expect(paneCount(min)).toBe(3);
    const restored = restorePane(min, b);
    expect(s(restored)).toBe("h(A,B,C)");
    expect(restored.minimized).toHaveLength(0);
    expect(restored.focusedId).toBe(b);
    expect(a).toBeTruthy();
  });

  it("restores the last pane in a split on the correct side", () => {
    const { doc, c, s } = three();
    expect(s(restorePane(minimizePane(doc, c), c))).toBe("h(A,B,C)");
  });

  it("falls back to the right edge when the anchor is gone", () => {
    const { doc, a, b, s } = three();
    const min = minimizePane(doc, a); // anchor = B
    const gone = closePane(min, b);
    expect(s(restorePane(gone, a))).toBe("h(C,A)");
  });

  it("handles minimizing the only pane", () => {
    const doc = addPane(docFromRoot(null), "explorer");
    const id = doc.root!.id;
    const min = minimizePane(doc, id);
    expect(min.root).toBeNull();
    expect(min.focusedId).toBeNull();
    expect(restorePane(min, id).root?.id).toBe(id);
  });

  it("counts minimized panes toward the cap and can discard them", () => {
    let doc = addPane(docFromRoot(null), "explorer");
    for (let i = 0; i < MAX_PANES; i++) doc = addPane(doc, "overview");
    const first = panes(doc.root)[0]!.id;
    const min = minimizePane(doc, first);
    expect(addPane(min, "explorer")).toBe(min);
    expect(splitPane(min, panes(min.root)[0]!.id, "vertical", "explorer")).toBe(min);
    expect(closePane(min, first).minimized).toHaveLength(0);
  });

  it("survives serialization", () => {
    const { doc, b } = three();
    const min = minimizePane(setPaneState(doc, b, { x: 1 }), b);
    const parsed = parseLayout(serializeLayout(min))!;
    expect(parsed.minimized[0]!.pane.state).toEqual({ x: 1 });
    expect(parseLayout(JSON.stringify({ ...min, minimized: [{ pane: { type: "pane", id: panes(min.root)[0]!.id, kind: "explorer" } }] }))).toBeNull();
  });
});

describe("templates", () => {
  it("instantiate gives fresh unique ids and drops state unless asked", () => {
    const { doc, a } = three();
    const withState = minimizePane(setPaneState(doc, a, { selected: "x" }), a);
    const plain = instantiate(withState);
    const ids = (d: LayoutDoc) => [...panes(d.root), ...d.minimized.map((m) => m.pane)].map((p) => p.id);
    expect(ids(plain).some((id) => ids(withState).includes(id))).toBe(false);
    expect(plain.minimized).toHaveLength(0);
    expect(panes(plain.root).every((p) => p.state === undefined)).toBe(true);

    const kept = instantiate(withState, true);
    expect(kept.minimized[0]!.pane.state).toEqual({ selected: "x" });
    const anchor = kept.minimized[0]!.anchorId!;
    expect(panes(kept.root).some((p) => p.id === anchor)).toBe(true);
    expect(new Set(ids(kept)).size).toBe(ids(kept).length);
  });

  it("toTemplate strips state and tray", () => {
    const { doc, a } = three();
    const t = toTemplate(minimizePane(setPaneState(doc, a, { y: 1 }), a));
    expect(t.minimized).toEqual([]);
    expect(panes(t.root).every((p) => !("state" in p))).toBe(true);
  });

  it("every built-in preset is a valid layout", () => {
    for (const p of BUILTIN_PRESETS) {
      const d = p.build();
      expect(parseLayout(serializeLayout(d)), p.name).not.toBeNull();
      expect(panes(d.root).length).toBeGreaterThan(0);
    }
  });
});

describe("ephemeral state", () => {
  it("copies never inherit a live terminal", () => {
    const t = createPane("terminal");
    const doc = setPaneState(docFromRoot(t), t.id, { terminalId: "pty-1", note: "keep" });
    const dup = duplicatePaneOf(doc, t.id);
    expect(panes(dup.root)[1]!.state).toEqual({ note: "keep" });
    const copy = instantiate(doc, true);
    expect(panes(copy.root)[0]!.state).toEqual({ note: "keep" });
    const onlyTerm = setPaneState(docFromRoot(t), t.id, { terminalId: "pty-1" });
    expect(panes(instantiate(onlyTerm, true).root)[0]!.state).toBeUndefined();
    expect(panes(onlyTerm.root)[0]!.state).toEqual({ terminalId: "pty-1" }); // original untouched
  });
});
