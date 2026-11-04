import { describe, expect, it } from "vitest";
import {
  MAX_PANES,
  addPane,
  closePane,
  cycleFocus,
  defaultLayout,
  emptyLayout,
  duplicatePane,
  normalizeSizes,
  panes,
  parseLayout,
  serializeLayout,
  setPaneKind,
  setPaneState,
  setSizes,
  splitPane,
  toggleMaximize,
  type LayoutDoc,
  createPane,
  docFromRoot,
  split,
  type SplitNode,
} from "./layout";

function twoPane(): LayoutDoc {
  return docFromRoot(split("horizontal", [createPane("explorer"), createPane("overview")], [62, 38]));
}

const sum = (xs: number[]) => Math.round(xs.reduce((a, b) => a + b, 0));

function single(): LayoutDoc {
  return addPane(emptyLayout(), "explorer");
}

describe("pane layout", () => {
  it("adds a root pane to an empty layout", () => {
    const doc = single();
    expect(doc.root?.type).toBe("pane");
    expect(doc.focusedId).toBe(doc.root?.id);
  });

  it("splits a leaf into a split node and focuses the new pane", () => {
    const doc = single();
    const id = doc.root!.id;
    const next = splitPane(doc, id, "vertical", "overview");
    const root = next.root as SplitNode;
    expect(root.type).toBe("split");
    expect(root.orientation).toBe("vertical");
    expect(root.children.map((c) => c.id)[0]).toBe(id);
    expect(next.focusedId).toBe(root.children[1]!.id);
    expect(sum(root.sizes)).toBe(100);
  });

  it("inserts into an existing split with the same orientation instead of nesting", () => {
    const doc = twoPane();
    const first = panes(doc.root)[0]!.id;
    const next = splitPane(doc, first, "horizontal", "explorer");
    const root = next.root as SplitNode;
    expect(root.children).toHaveLength(3);
    expect(sum(root.sizes)).toBe(100);
  });

  it("nests when splitting with a different orientation", () => {
    const doc = twoPane();
    const first = panes(doc.root)[0]!.id;
    const root = splitPane(doc, first, "vertical", "overview").root as SplitNode;
    expect(root.children).toHaveLength(2);
    expect((root.children[0] as SplitNode).orientation).toBe("vertical");
  });

  it("closing a pane collapses single-child splits", () => {
    const doc = twoPane();
    const [a, b] = panes(doc.root);
    const next = closePane(doc, a!.id);
    expect(next.root?.type).toBe("pane");
    expect(next.root?.id).toBe(b!.id);
    expect(next.focusedId).toBe(b!.id);
    const empty = closePane(next, b!.id);
    expect(empty.root).toBeNull();
    expect(empty.focusedId).toBeNull();
  });

  it("caps the number of panes", () => {
    let doc = single();
    for (let i = 0; i < MAX_PANES + 5; i++) doc = addPane(doc, "overview");
    expect(panes(doc.root)).toHaveLength(MAX_PANES);
  });

  it("maximize toggles and is cleared when the pane closes", () => {
    const doc = twoPane();
    const id = panes(doc.root)[0]!.id;
    const max = toggleMaximize(doc, id);
    expect(max.maximizedId).toBe(id);
    expect(toggleMaximize(max, id).maximizedId).toBeNull();
    expect(closePane(max, id).maximizedId).toBeNull();
  });

  it("changes kind (dropping old state) and stores pane state", () => {
    const doc = twoPane();
    const id = panes(doc.root)[0]!.id;
    const withState = setPaneState(doc, id, { expanded: ["src"] });
    expect(panes(withState.root)[0]!.state).toEqual({ expanded: ["src"] });
    const changed = setPaneKind(withState, id, "overview");
    expect(panes(changed.root)[0]!.kind).toBe("overview");
    expect(panes(changed.root)[0]!.state).toBeUndefined();
  });

  it("duplicates a pane with a copy of its state", () => {
    const base = single();
    const id = base.root!.id;
    const withState = setPaneState(base, id, { selected: "a.ts" });
    const dup = duplicatePane(withState, id);
    const list = panes(dup.root);
    expect(list).toHaveLength(2);
    expect(list[1]!.kind).toBe("explorer");
    expect(list[1]!.state).toEqual({ selected: "a.ts" });
    expect(list[1]!.state).not.toBe(list[0]!.state);
  });

  it("normalizes sizes", () => {
    expect(sum(normalizeSizes([1, 1, 2], 3))).toBe(100);
    expect(normalizeSizes([50], 2)).toHaveLength(2);
    expect(normalizeSizes([NaN, 10], 2).every((s) => s > 0)).toBe(true);
    const doc = twoPane();
    const resized = setSizes(doc, doc.root!.id, [30, 70]);
    expect((resized.root as SplitNode).sizes).toEqual([30, 70]);
  });

  it("cycles focus", () => {
    const doc = twoPane();
    const [a, b] = panes(doc.root);
    const f = { ...doc, focusedId: a!.id };
    expect(cycleFocus(f, 1).focusedId).toBe(b!.id);
    expect(cycleFocus(f, -1).focusedId).toBe(b!.id);
  });

  it("round-trips through serialization and rejects garbage", () => {
    const doc = twoPane();
    expect(parseLayout(serializeLayout(doc))).toEqual(doc);
    expect(parseLayout(null)).toBeNull();
    expect(parseLayout("nope")).toBeNull();
    expect(parseLayout(JSON.stringify({ version: 2, root: null }))).toBeNull();
    expect(parseLayout(JSON.stringify({ version: 1, root: { type: "pane", id: "x", kind: "bogus" } }))).toBeNull();
    const dupIds = { version: 1, root: { type: "split", id: "s", orientation: "horizontal", sizes: [50, 50], children: [{ type: "pane", id: "a", kind: "explorer" }, { type: "pane", id: "a", kind: "explorer" }] } };
    expect(parseLayout(JSON.stringify(dupIds))).toBeNull();
  });

  it("repairs dangling focus/maximize references on load", () => {
    const doc = { ...twoPane(), focusedId: "ghost", maximizedId: "ghost" };
    const parsed = parseLayout(JSON.stringify(doc))!;
    expect(parsed.focusedId).toBe(panes(parsed.root)[0]!.id);
    expect(parsed.maximizedId).toBeNull();
  });
});

describe("default layout", () => {
  it("opens files, editor and overview side by side", () => {
    const doc = defaultLayout();
    expect(panes(doc.root).map((p) => p.kind)).toEqual(["explorer", "editor", "overview"]);
    expect(parseLayout(serializeLayout(doc))).toEqual(doc);
  });
});
