import { describe, expect, it } from "vitest";
import { agentDraft, extractLocalUrl, keyEvents, mapPoint, modifiersOf } from "./input";

const k = (key: string, extra: Partial<{ code: string; keyCode: number; altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }> = {}) => ({
  key,
  code: extra.code ?? "",
  keyCode: extra.keyCode ?? 0,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  ...extra,
});

describe("browser input helpers", () => {
  it("builds the CDP modifier mask", () => {
    expect(modifiersOf({ altKey: true, ctrlKey: false, metaKey: true, shiftKey: true })).toBe(13);
    expect(modifiersOf({ altKey: false, ctrlKey: false, metaKey: false, shiftKey: false })).toBe(0);
  });

  it("maps pointer positions through object-contain letterboxing", () => {
    // 800x400 box showing a 400x400 page → 200px bars on each side, scale 1.
    const rect = { left: 0, top: 0, width: 800, height: 400 };
    expect(mapPoint(200, 0, rect, 400, 400)).toEqual({ x: 0, y: 0 });
    expect(mapPoint(400, 200, rect, 400, 400)).toEqual({ x: 200, y: 200 });
    expect(mapPoint(100, 200, rect, 400, 400)).toBeNull();
    // Downscaled: 1280x800 page in a 640x400 box → scale 0.5.
    expect(mapPoint(320, 200, { left: 10, top: 10, width: 640, height: 400 }, 1280, 800)).toEqual({ x: 620, y: 380 });
    expect(mapPoint(1, 1, rect, 0, 0)).toBeNull();
  });

  it("translates keys: printable text, Enter and special keys", () => {
    expect(keyEvents("down", k("a", { code: "KeyA", keyCode: 65 }))).toEqual([{ kind: "keyDown", key: "a", code: "KeyA", keyCode: 65, modifiers: 0, text: "a" }]);
    expect(keyEvents("down", k("Enter", { keyCode: 13 }))[0]).toMatchObject({ kind: "keyDown", text: "\r" });
    expect(keyEvents("down", k("Backspace", { keyCode: 8 }))[0]).toMatchObject({ kind: "rawKeyDown", keyCode: 8 });
    expect(keyEvents("down", k("a", { metaKey: true }))[0]).toMatchObject({ kind: "rawKeyDown", modifiers: 4 });
    expect(keyEvents("up", k("a"))[0]!.kind).toBe("keyUp");
  });

  it("extracts local URLs printed by dev servers", () => {
    expect(extractLocalUrl("\u001b[32m  ➜  Local:   \u001b[36mhttp://localhost:\u001b[1m5173\u001b[22m/\u001b[39m")).toBe("http://localhost:5173/");
    expect(extractLocalUrl("   - Local:        http://localhost:3000\n")).toBe("http://localhost:3000");
    expect(extractLocalUrl("Server listening on http://0.0.0.0:8080.")).toBe("http://localhost:8080");
    expect(extractLocalUrl("ready at http://127.0.0.1:4321/docs, press h")).toBe("http://127.0.0.1:4321/docs");
    expect(extractLocalUrl("see https://vitejs.dev for docs")).toBeNull();
    expect(extractLocalUrl("compiling…")).toBeNull();
  });

  it("frames verify reports as untrusted data for agents", () => {
    const d = agentDraft("## Verificação\n- Erros: 1\n");
    expect(d).toContain("não confiáveis");
    expect(d.endsWith("- Erros: 1")).toBe(true);
  });
});
