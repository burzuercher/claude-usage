// Burn-rate sparkline with a "safe pace" threshold line.
export function BurnSpark({ data, threshold }: { data: number[]; threshold: number }) {
  const W = 360, H = 64;
  const max = Math.max(...data, threshold, 1) * 1.15;
  const stepX = W / Math.max(1, data.length - 1);
  const pts = data.map((v, i) => [i * stepX, H - (v / max) * (H - 6) - 3] as const);
  const path = pts.map((p, i) => (i ? `L${p[0]},${p[1]}` : `M${p[0]},${p[1]}`)).join(" ");
  const area = `${path} L${W},${H} L0,${H} Z`;
  const thresholdY = H - (threshold / max) * (H - 6) - 3;
  const last = pts[pts.length - 1] ?? [0, H];

  return (
    <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} className="spark">
      <defs>
        <linearGradient id="sparkFill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--clay)" stopOpacity="0.34" />
          <stop offset="100%" stopColor="var(--clay)" stopOpacity="0" />
        </linearGradient>
      </defs>
      <line x1="0" y1={thresholdY} x2={W} y2={thresholdY} stroke="var(--ink-30)" strokeWidth="1" strokeDasharray="3 3" />
      <text x={W - 4} y={thresholdY - 4} textAnchor="end" fontSize="9" fill="var(--ink-60)" className="mono">
        safe pace
      </text>
      <path d={area} fill="url(#sparkFill)" />
      <path d={path} stroke="var(--clay)" strokeWidth="1.75" fill="none" strokeLinejoin="round" strokeLinecap="round" />
      <circle cx={last[0]} cy={last[1]} r="3" fill="var(--clay)" stroke="var(--paper)" strokeWidth="1.5" />
      <circle cx={last[0]} cy={last[1]} r="6" fill="var(--clay)" opacity="0.25" />
    </svg>
  );
}
