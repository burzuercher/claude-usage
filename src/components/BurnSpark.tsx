// Session burn-rate sparkline: tokens per 15-minute bin across the whole
// 5-hour window (x axis = window start → reset). The line is drawn only up to
// "now"; the remaining part of the window is shaded. The dashed "safe pace"
// line is the per-bin token rate that would land exactly at 100% at reset.
export function BurnSpark({
  data,
  threshold,
  nowBins,
}: {
  /** Tokens per bin, indexed from the window start (20 bins). */
  data: number[];
  /** Safe tokens-per-bin (0 hides the line). */
  threshold: number;
  /** Bins elapsed so far (fractional, 0..data.length). */
  nowBins: number;
}) {
  const W = 360, H = 64;
  const n = Math.max(1, data.length);
  const stepX = W / n;
  const nowIdx = Math.min(n - 1, Math.max(0, Math.floor(nowBins)));
  const nowX = Math.min(W, Math.max(0, nowBins * stepX));
  const shown = data.slice(0, nowIdx + 1);
  const max = Math.max(...shown, threshold, 1) * 1.15;
  const y = (v: number) => H - (v / max) * (H - 8) - 3;
  // Each bin plotted at its centre; the current (partial) bin at "now".
  const pts = shown.map((v, i) => [i === nowIdx ? nowX : (i + 0.5) * stepX, y(v)] as const);
  const path = pts.map((p, i) => (i ? `L${p[0]},${p[1]}` : `M${p[0]},${p[1]}`)).join(" ");
  const area = pts.length > 1 ? `${path} L${pts[pts.length - 1][0]},${H} L${pts[0][0]},${H} Z` : "";
  const thresholdY = y(threshold);
  const last = pts[pts.length - 1] ?? [nowX, H];

  return (
    <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} className="spark">
      <defs>
        <linearGradient id="sparkFill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--clay)" stopOpacity="0.34" />
          <stop offset="100%" stopColor="var(--clay)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {/* remaining part of the window */}
      <rect x={nowX} y="0" width={Math.max(0, W - nowX)} height={H} fill="var(--ink-10)" opacity="0.6" />
      <line x1={nowX} y1="0" x2={nowX} y2={H} stroke="var(--ink-30)" strokeWidth="1" />
      {threshold > 0 && (
        <>
          <line x1="0" y1={thresholdY} x2={W} y2={thresholdY} stroke="var(--ink-30)" strokeWidth="1" strokeDasharray="3 3" />
          <text x={W - 4} y={thresholdY - 4} textAnchor="end" fontSize="9" fill="var(--ink-60)" className="mono">
            safe pace
          </text>
        </>
      )}
      {area && <path d={area} fill="url(#sparkFill)" />}
      <path d={path} stroke="var(--clay)" strokeWidth="1.75" fill="none" strokeLinejoin="round" strokeLinecap="round" />
      <circle cx={last[0]} cy={last[1]} r="3" fill="var(--clay)" stroke="var(--paper)" strokeWidth="1.5" />
      <circle cx={last[0]} cy={last[1]} r="6" fill="var(--clay)" opacity="0.25" />
      <text x="3" y={H - 2} fontSize="8" fill="var(--ink-40)" className="mono">start</text>
      <text x={W - 3} y={H - 2} textAnchor="end" fontSize="8" fill="var(--ink-40)" className="mono">reset</text>
    </svg>
  );
}
