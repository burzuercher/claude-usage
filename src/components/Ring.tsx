import { useCountUp } from "../hooks/useCountUp";

// Big 5-hour session progress ring. `value` is 0..1.
export function Ring({ value, resetIn }: { value: number; resetIn: string }) {
  const animated = useCountUp(value, 1100);
  const size = 188;
  const stroke = 14;
  const r = (size - stroke) / 2;
  const C = 2 * Math.PI * r;
  const dash = C * animated;
  const pct = Math.round(animated * 100);

  // Color ramp as usage climbs.
  const color = animated < 0.6 ? "var(--clay)" : animated < 0.85 ? "var(--amber)" : "var(--ember)";

  return (
    <div className="ring-wrap">
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="ring">
        <defs>
          <linearGradient id="ringGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor={color} stopOpacity="1" />
            <stop offset="100%" stopColor={color} stopOpacity="0.7" />
          </linearGradient>
        </defs>
        <circle cx={size / 2} cy={size / 2} r={r} stroke="var(--track)" strokeWidth={stroke} fill="none" />
        <circle
          cx={size / 2} cy={size / 2} r={r}
          stroke="url(#ringGrad)" strokeWidth={stroke} fill="none"
          strokeLinecap="round"
          strokeDasharray={`${dash} ${C}`}
          style={{ transition: "stroke 600ms ease" }}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
        {Array.from({ length: 20 }).map((_, i) => {
          const a = (i / 20) * Math.PI * 2 - Math.PI / 2;
          const inner = r - stroke / 2 - 4;
          const outer = r - stroke / 2 - (i % 5 === 0 ? 9 : 6);
          const x1 = size / 2 + Math.cos(a) * inner;
          const y1 = size / 2 + Math.sin(a) * inner;
          const x2 = size / 2 + Math.cos(a) * outer;
          const y2 = size / 2 + Math.sin(a) * outer;
          return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} stroke="var(--ink-30)" strokeWidth="1" />;
        })}
      </svg>
      <div className="ring-center">
        <div className="ring-label">5-hour session</div>
        <div className="ring-pct">
          <span className="ring-num">{pct}</span>
          <span className="ring-pct-sym">%</span>
        </div>
        <div className="ring-sub">
          <span className="dot" style={{ background: color }} />
          resets in <b>{resetIn}</b>
        </div>
      </div>
    </div>
  );
}
