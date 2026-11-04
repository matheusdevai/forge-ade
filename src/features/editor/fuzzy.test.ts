import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyScore } from "./fuzzy";

const FILES = [
  "src/features/panes/layout.ts",
  "src/features/panes/layout.test.ts",
  "src/features/panes/PaneFrame.tsx",
  "src/lib/ipc.ts",
  "src/App.tsx",
  "src-tauri/src/lib.rs",
  "docs/ARCHITECTURE.md",
  "package.json",
];

describe("fuzzy", () => {
  it("requires characters in order", () => {
    expect(fuzzyScore("ipc", "src/lib/ipc.ts")).not.toBeNull();
    expect(fuzzyScore("cpi", "src/lib/ipc.ts")).toBeNull();
    expect(fuzzyScore("", "x")?.score).toBe(0);
  });

  it("ranks exact file names and segment starts first", () => {
    expect(fuzzyFilter("ipc", FILES)[0]!.path).toBe("src/lib/ipc.ts");
    expect(fuzzyFilter("layout", FILES)[0]!.path).toBe("src/features/panes/layout.ts");
    expect(fuzzyFilter("pf", FILES)[0]!.path).toBe("src/features/panes/PaneFrame.tsx");
    expect(fuzzyFilter("pkg json", FILES).map((r) => r.path)).toContain("package.json");
  });

  it("returns match positions for highlighting", () => {
    const r = fuzzyScore("app", "src/App.tsx")!;
    expect(r.positions.map((i) => "src/App.tsx"[i]).join("").toLowerCase()).toBe("app");
  });

  it("limits results", () => {
    expect(fuzzyFilter("s", FILES, 3)).toHaveLength(3);
  });
});
