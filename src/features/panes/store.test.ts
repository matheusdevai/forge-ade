import { describe, expect, it } from "vitest";
import { createPane, docFromRoot, minimizePane, setPaneState, split } from "./layout";
import { referencedTerminalIds } from "./store";

describe("referencedTerminalIds", () => {
  it("collects terminal ids from every session, including minimized panes", () => {
    const [t1, t2, ex] = [createPane("terminal"), createPane("terminal"), createPane("explorer")];
    let a = docFromRoot(split("horizontal", [t1, t2, ex]));
    a = setPaneState(a, t1.id, { terminalId: "one" });
    a = setPaneState(a, t2.id, { terminalId: "two" });
    a = setPaneState(a, ex.id, { terminalId: "not-a-terminal" });
    a = minimizePane(a, t2.id);
    const t3 = createPane("terminal");
    const b = setPaneState(docFromRoot(t3), t3.id, { terminalId: "three" });
    const fresh = docFromRoot(createPane("terminal")); // not spawned yet
    expect([...referencedTerminalIds([a, b, fresh])].sort()).toEqual(["one", "three", "two"]);
  });
});

describe("agent panes", () => {
  it("count as terminal owners for garbage collection", () => {
    const c = createPane("claude");
    const g = createPane("gemini");
    let d = docFromRoot(split("horizontal", [c, g]));
    d = setPaneState(d, c.id, { terminalId: "claude-pty" });
    d = setPaneState(d, g.id, { terminalId: "gemini-pty", launchMode: "login" });
    expect([...referencedTerminalIds([d])].sort()).toEqual(["claude-pty", "gemini-pty"]);
  });
});
