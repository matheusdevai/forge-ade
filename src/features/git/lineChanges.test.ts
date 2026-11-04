import { describe, expect, it } from "vitest";
import { rebuild, touches, type LineChange } from "./lineChanges";

const ch = (os: number, oe: number, ms: number, me: number): LineChange => ({
  originalStartLineNumber: os,
  originalEndLineNumber: oe,
  modifiedStartLineNumber: ms,
  modifiedEndLineNumber: me,
});

// index:    1 2 3 _        worktree: 1 X 3 4 _
const INDEX = "1\n2\n3\n";
const WORK = "1\nX\n3\n4\n";
const MOD = ch(2, 2, 2, 2); // line 2 changed
const ADD = ch(3, 0, 4, 4); // line 4 inserted after original line 3

describe("partial staging", () => {
  it("stages only the picked change", () => {
    expect(rebuild(INDEX, WORK, [MOD, ADD], (c) => c === MOD, "original")).toBe("1\nX\n3\n");
    expect(rebuild(INDEX, WORK, [MOD, ADD], (c) => c === ADD, "original")).toBe("1\n2\n3\n4\n");
    expect(rebuild(INDEX, WORK, [MOD, ADD], () => true, "original")).toBe(WORK);
    expect(rebuild(INDEX, WORK, [MOD, ADD], () => false, "original")).toBe(INDEX);
  });

  it("unstages / discards by reverting picked changes on the modified side", () => {
    // base = worktree (modified), revert only the insertion
    expect(rebuild(WORK, INDEX, [MOD, ADD], (c) => c === ADD, "modified")).toBe("1\nX\n3\n");
    expect(rebuild(WORK, INDEX, [MOD, ADD], () => true, "modified")).toBe(INDEX);
  });

  it("handles deletions", () => {
    // original: a b c d / modified: a d   → lines 2-3 deleted after modified line 1
    const DEL = ch(2, 3, 1, 0);
    expect(rebuild("a\nb\nc\nd", "a\nd", [DEL], () => true, "original")).toBe("a\nd");
    expect(rebuild("a\nd", "a\nb\nc\nd", [DEL], () => true, "modified")).toBe("a\nb\nc\nd");
  });

  it("preserves CRLF line endings", () => {
    expect(rebuild("1\r\n2\r\n", "1\r\nX\r\n", [ch(2, 2, 2, 2)], () => true, "original")).toBe("1\r\nX\r\n");
  });

  it("selection overlap", () => {
    expect(touches(MOD, [{ start: 2, end: 2 }], "modified")).toBe(true);
    expect(touches(MOD, [{ start: 3, end: 5 }], "modified")).toBe(false);
    expect(touches(ADD, [{ start: 4, end: 4 }], "modified")).toBe(true);
    const DEL = ch(2, 3, 1, 0);
    expect(touches(DEL, [{ start: 1, end: 1 }], "modified")).toBe(true);
    expect(touches(DEL, [{ start: 2, end: 2 }], "modified")).toBe(true);
    expect(touches(DEL, [{ start: 4, end: 9 }], "modified")).toBe(false);
  });
});
