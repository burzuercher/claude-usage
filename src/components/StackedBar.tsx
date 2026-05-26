export interface Segment {
  id: string;
  share: number; // 0..1
  color: string;
}

// Segmented horizontal bar (model / surface split).
export function StackedBar({ segments, height = 10 }: { segments: Segment[]; height?: number }) {
  return (
    <div className="stack-bar" style={{ height }}>
      {segments.map((s, i) => (
        <div
          key={s.id}
          className="stack-seg"
          style={{ width: `${s.share * 100}%`, background: s.color, transitionDelay: `${i * 60}ms` }}
        />
      ))}
    </div>
  );
}
