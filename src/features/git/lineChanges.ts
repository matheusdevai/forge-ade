/**
 * Partial staging by line changes (Monaco `ILineChange` semantics):
 * an `*EndLineNumber` of 0 means an empty range positioned *after* `*StartLineNumber`.
 *
 * `rebuild(base, other, changes, pick, baseSide)` returns `base` with the picked
 * changes replaced by the corresponding lines of `other`:
 *  - stage selection:   base = index (original),     other = worktree (modified) → new index
 *  - unstage selection: base = index (modified),     other = HEAD (original)     → new index
 *  - discard selection: base = worktree (modified),  other = index (original)    → new worktree
 */
export type LineChange = {
  originalStartLineNumber: number;
  originalEndLineNumber: number;
  modifiedStartLineNumber: number;
  modifiedEndLineNumber: number;
};
export type Side = "original" | "modified";
export type LineRange = { start: number; end: number };

function range(start: number, end: number): [number, number] {
  return end === 0 ? [start + 1, start] : [start, end];
}

function sideRange(c: LineChange, side: Side): [number, number] {
  return side === "original"
    ? range(c.originalStartLineNumber, c.originalEndLineNumber)
    : range(c.modifiedStartLineNumber, c.modifiedEndLineNumber);
}

export function rebuild(base: string, other: string, changes: LineChange[], pick: (c: LineChange) => boolean, baseSide: Side): string {
  const otherSide: Side = baseSide === "original" ? "modified" : "original";
  const baseLines = base.split("\n");
  const otherLines = other.split("\n");
  const sorted = [...changes].sort((a, b) => sideRange(a, baseSide)[0] - sideRange(b, baseSide)[0]);
  const out: string[] = [];
  let next = 1;
  for (const c of sorted) {
    const [bs, be] = sideRange(c, baseSide);
    const [os, oe] = sideRange(c, otherSide);
    out.push(...baseLines.slice(next - 1, bs - 1));
    if (pick(c)) out.push(...otherLines.slice(os - 1, oe));
    else out.push(...baseLines.slice(bs - 1, be));
    next = be + 1;
  }
  out.push(...baseLines.slice(next - 1));
  return out.join("\n");
}

/** True when the change touches any selected line range on `side` (deletions count at their anchor). */
export function touches(c: LineChange, selections: LineRange[], side: Side): boolean {
  const [s, e] = sideRange(c, side);
  const lo = Math.min(s, e + 1) - (s > e ? 1 : 0);
  const hi = Math.max(s, e);
  return selections.some((r) => r.start <= hi && r.end >= lo);
}
