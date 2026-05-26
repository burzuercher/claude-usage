import { useEffect, useRef, useState } from "react";

// Animated number — eases from the previous value to `target` (easeOutCubic).
// Ported from the prototype's widget.jsx.
export function useCountUp(target: number, duration = 900): number {
  const [val, setVal] = useState(0);
  const startRef = useRef<number | null>(null);
  const fromRef = useRef(0);
  const valRef = useRef(0);
  valRef.current = val;

  useEffect(() => {
    fromRef.current = valRef.current;
    startRef.current = null;
    let raf = 0;
    const tick = (t: number) => {
      if (startRef.current === null) startRef.current = t;
      const p = Math.min(1, (t - startRef.current) / duration);
      const e = 1 - Math.pow(1 - p, 3); // easeOutCubic
      setVal(fromRef.current + (target - fromRef.current) * e);
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, duration]);

  return val;
}
