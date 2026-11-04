import { beforeEach, describe, expect, it } from "vitest";
import { createPane, docFromRoot, minimizePane, panes, setPaneState, split } from "@/features/panes/layout";
import { useLayout } from "@/features/panes/store";
import { closeTabsUnder, openFileInEditor, readEditorState, referencedFiles, renameTabs, takeReveal } from "./actions";

function load(doc: ReturnType<typeof docFromRoot>, extra: Record<string, ReturnType<typeof docFromRoot>> = {}) {
  const docs = { s1: doc, ...extra };
  useLayout.setState({ workspaceId: "w", sessions: Object.keys(docs).map((id) => ({ id, name: id })), activeId: "s1", docs, doc });
}
const editorState = () => {
  const ed = panes(useLayout.getState().doc!.root).find((p) => p.kind === "editor");
  return ed ? readEditorState(ed.state) : null;
};

describe("editor actions", () => {
  beforeEach(() => load(docFromRoot(split("horizontal", [createPane("explorer"), createPane("overview")]))));

  it("creates an editor pane when none exists and opens the file", () => {
    openFileInEditor("src/a.ts");
    const doc = useLayout.getState().doc!;
    expect(panes(doc.root).map((p) => p.kind)).toContain("editor");
    expect(editorState()).toEqual({ tabs: ["src/a.ts"], active: "src/a.ts" });
    expect(doc.focusedId).toBe(panes(doc.root).find((p) => p.kind === "editor")!.id);
  });

  it("reuses the editor, de-duplicates tabs and queues reveals", () => {
    openFileInEditor("a.ts");
    openFileInEditor("b.ts", { line: 3, column: 2 });
    openFileInEditor("a.ts");
    expect(editorState()).toEqual({ tabs: ["a.ts", "b.ts"], active: "a.ts" });
    expect(panes(useLayout.getState().doc!.root).filter((p) => p.kind === "editor")).toHaveLength(1);
    const id = panes(useLayout.getState().doc!.root).find((p) => p.kind === "editor")!.id;
    expect(takeReveal(id, "a.ts")).toBeNull();
    expect(takeReveal(id, "b.ts")).toEqual({ line: 3, column: 2 });
    expect(takeReveal(id, "b.ts")).toBeNull();
  });

  it("restores a minimized editor instead of creating another", () => {
    const ed = createPane("editor");
    let doc = docFromRoot(split("horizontal", [createPane("explorer"), ed]));
    doc = minimizePane(doc, ed.id);
    load(doc);
    openFileInEditor("x.md");
    const after = useLayout.getState().doc!;
    expect(after.minimized).toHaveLength(0);
    expect(panes(after.root).find((p) => p.id === ed.id)).toBeTruthy();
  });

  it("renames and closes tabs across every session, including folders", () => {
    const e1 = createPane("editor");
    const e2 = createPane("editor");
    const s1 = setPaneState(docFromRoot(e1), e1.id, { tabs: ["src/a.ts", "src/lib/b.ts", "README.md"], active: "src/lib/b.ts" });
    const s2 = minimizePane(setPaneState(docFromRoot(split("horizontal", [createPane("explorer"), e2])), e2.id, { tabs: ["src/lib/b.ts"], active: "src/lib/b.ts" }), e2.id);
    load(s1, { s2 });
    renameTabs("src/lib", "src/core");
    const docs = useLayout.getState().docs;
    expect(readEditorState(panes(docs.s1!.root)[0]!.state)).toEqual({ tabs: ["src/a.ts", "src/core/b.ts", "README.md"], active: "src/core/b.ts" });
    expect(readEditorState(docs.s2!.minimized[0]!.pane.state).tabs).toEqual(["src/core/b.ts"]);
    expect([...referencedFiles()].sort()).toEqual(["README.md", "src/a.ts", "src/core/b.ts"]);

    closeTabsUnder("src");
    expect(readEditorState(panes(useLayout.getState().docs.s1!.root)[0]!.state)).toEqual({ tabs: ["README.md"], active: "README.md" });
    expect(readEditorState(useLayout.getState().docs.s2!.minimized[0]!.pane.state).tabs).toEqual([]);
  });

  it("readEditorState sanitizes garbage", () => {
    expect(readEditorState({ tabs: ["a", 3, "a", "b"], active: "zzz" })).toEqual({ tabs: ["a", "b"], active: "b" });
    expect(readEditorState(undefined)).toEqual({ tabs: [], active: null });
  });
});
