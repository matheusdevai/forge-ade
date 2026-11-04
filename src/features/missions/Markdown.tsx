/** Minimal markdown renderer for FORGE-generated reports (headings, lists, bold, inline code). */
function inline(text: string, key: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const tok = m[0];
    out.push(
      tok.startsWith("**") ? (
        <strong key={`${key}-${i++}`} className="font-semibold text-fg">
          {tok.slice(2, -2)}
        </strong>
      ) : (
        <code key={`${key}-${i++}`} className="rounded bg-bg px-1 font-mono text-[11.5px] text-fg">
          {tok.slice(1, -1)}
        </code>
      ),
    );
    last = m.index + tok.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

export function Markdown({ text }: { text: string }) {
  return (
    <div className="selectable space-y-1 text-[12.5px] leading-relaxed text-muted">
      {text.split("\n").map((line, i) => {
        const k = String(i);
        if (line.startsWith("### ")) return <h4 key={k} className="pt-2 text-[13px] font-semibold text-fg">{inline(line.slice(4), k)}</h4>;
        if (line.startsWith("## ")) return <h3 key={k} className="pt-3 text-[13.5px] font-semibold text-fg">{inline(line.slice(3), k)}</h3>;
        if (line.startsWith("# ")) return <h2 key={k} className="text-[15px] font-semibold text-fg">{inline(line.slice(2), k)}</h2>;
        const bullet = /^(\s*)- (.*)$/.exec(line);
        if (bullet) {
          const depth = Math.floor(bullet[1]!.length / 2);
          return (
            <div key={k} className="flex gap-2" style={{ paddingLeft: depth * 16 }}>
              <span className="text-subtle">•</span>
              <span className="min-w-0">{inline(bullet[2]!, k)}</span>
            </div>
          );
        }
        if (!line.trim()) return <div key={k} className="h-1" />;
        return <p key={k}>{inline(line, k)}</p>;
      })}
    </div>
  );
}
