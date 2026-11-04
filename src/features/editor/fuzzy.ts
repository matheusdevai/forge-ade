/**
 * Fuzzy path matcher for Quick Open. Characters of the query must appear in
 * order; the score rewards consecutive runs, matches at word/segment starts
 * and matches in the file name over the directory.
 */
export type FuzzyResult = { path: string; score: number; positions: number[] };

const SEPARATORS = new Set(["/", "\\", "_", "-", ".", " "]);

export function fuzzyScore(query: string, path: string): FuzzyResult | null {
  const q = query.replace(/\s+/g, "").toLowerCase();
  if (!q) return { path, score: 0, positions: [] };
  const lower = path.toLowerCase();
  const baseStart = path.lastIndexOf("/") + 1;

  // Prefer matching inside the basename: try a basename-anchored pass first.
  const attempt = (from: number): FuzzyResult | null => {
    const positions: number[] = [];
    let score = 0;
    let qi = 0;
    let prev = -2;
    for (let i = from; i < lower.length && qi < q.length; i++) {
      if (lower[i] !== q[qi]) continue;
      let s = 1;
      if (i === prev + 1) s += 5; // consecutive
      const before = path[i - 1];
      if (i === 0 || (before !== undefined && SEPARATORS.has(before))) s += 8; // segment start
      else if (before !== undefined && /[a-z]/.test(before) && /[A-Z]/.test(path[i]!)) s += 6; // camelCase hump
      if (i >= baseStart) s += 3; // in file name
      score += s;
      positions.push(i);
      prev = i;
      qi++;
    }
    if (qi < q.length) return null;
    // Shorter paths and exact basename hits rank higher.
    score -= Math.min(20, path.length / 10);
    const base = lower.slice(baseStart);
    if (base === q || base.startsWith(q + ".")) score += 25;
    return { path, score, positions };
  };

  const inBase = attempt(baseStart);
  const anywhere = attempt(0);
  if (!inBase) return anywhere;
  if (!anywhere) return inBase;
  return inBase.score >= anywhere.score ? inBase : anywhere;
}

export function fuzzyFilter(query: string, paths: string[], limit = 50): FuzzyResult[] {
  const out: FuzzyResult[] = [];
  for (const p of paths) {
    const r = fuzzyScore(query, p);
    if (r) out.push(r);
  }
  out.sort((a, b) => b.score - a.score || a.path.length - b.path.length || a.path.localeCompare(b.path));
  return out.slice(0, limit);
}
