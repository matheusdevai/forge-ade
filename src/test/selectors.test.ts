/**
 * Static guard against unstable zustand selectors: returning a fresh array or
 * object from `useStore(selector)` re-renders forever (React error #185).
 * Select the stable value and derive with useMemo instead.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : /\.(ts|tsx)$/.test(f) && !/\.test\.tsx?$/.test(f) ? [p] : [];
  });
}

// `useX((s) => …)` on one line, where the body creates a new array/object.
const SELECTOR = /\buse[A-Z]\w*\(\s*\(?\s*(\w+)\s*\)?\s*=>\s*([^;\n]*)\)/g;
const UNSTABLE = [
  /\.(filter|map|slice|concat|sort|toSorted|flatMap)\(/, // builds a new array
  /\?\?\s*(\[\]|\{\})/, // fresh default
  /\|\|\s*(\[\]|\{\})/,
  /^\s*\(\s*\{/, // object literal
  /^\s*\[/, // array literal
  /Object\.(keys|values|entries)\(/,
];

describe("zustand selectors are stable", () => {
  it("no selector returns a new array/object on every read", () => {
    const offenders: string[] = [];
    for (const f of files(join(__dirname, ".."))) {
      const src = readFileSync(f, "utf8");
      for (const m of src.matchAll(SELECTOR)) {
        const body = m[2] ?? "";
        // `.length` / `.find(...)` etc. return stable values.
        if (/\.length\s*\)?\s*$/.test(body)) continue;
        if (UNSTABLE.some((re) => re.test(body))) offenders.push(`${f.split("/src/")[1]}: ${m[0]}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
