// SVG icon set ported from the prototype's widget.jsx.

export type IconName =
  | "pin" | "min" | "max" | "x" | "code" | "chat" | "cowork"
  | "spark" | "down" | "gear" | "refresh";

export function Icon({ name, size = 14 }: { name: IconName; size?: number }) {
  const s = { width: size, height: size, display: "inline-block", verticalAlign: "-2px" } as const;
  switch (name) {
    case "pin":
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none">
          <path d="M10 1.5 14.5 6 11 7l-1 5-2-2-4 4 4-4-2-2 5-1z"
            stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" fill="currentColor" fillOpacity="0.18" />
        </svg>
      );
    case "min":
      return <svg style={s} viewBox="0 0 16 16"><path d="M3 8h10" stroke="currentColor" strokeWidth="1.1" /></svg>;
    case "max":
      return <svg style={s} viewBox="0 0 16 16"><rect x="3.5" y="3.5" width="9" height="9" stroke="currentColor" strokeWidth="1.1" fill="none" /></svg>;
    case "x":
      return <svg style={s} viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.1" /></svg>;
    case "code":
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none">
          <path d="M6 4 2 8l4 4M10 4l4 4-4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      );
    case "chat":
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none">
          <path d="M3 4h10v7H7l-3 3v-3H3z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
        </svg>
      );
    case "cowork":
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none">
          <circle cx="6" cy="6" r="2.2" stroke="currentColor" strokeWidth="1.2" />
          <circle cx="11" cy="9" r="1.8" stroke="currentColor" strokeWidth="1.2" />
          <path d="M2 13c.5-2 2-3 4-3M8 13c.4-1.4 1.4-2.2 3-2.2" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
        </svg>
      );
    case "spark":
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none">
          <path d="M8 1.5 9.4 6 14 7.5 9.4 9 8 13.5 6.6 9 2 7.5 6.6 6z" fill="currentColor" />
        </svg>
      );
    case "down":
      return <svg style={s} viewBox="0 0 16 16"><path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.2" fill="none" strokeLinecap="round" strokeLinejoin="round" /></svg>;
    case "gear":
      // Sliders/controls glyph — clearer "settings" than a cog at small sizes.
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round">
          <line x1="2.5" y1="4.5" x2="13.5" y2="4.5" />
          <line x1="2.5" y1="8" x2="13.5" y2="8" />
          <line x1="2.5" y1="11.5" x2="13.5" y2="11.5" />
          <circle cx="5.5" cy="4.5" r="1.7" fill="var(--win-bg)" />
          <circle cx="10.5" cy="8" r="1.7" fill="var(--win-bg)" />
          <circle cx="7" cy="11.5" r="1.7" fill="var(--win-bg)" />
        </svg>
      );
    case "refresh":
      return (
        <svg style={s} viewBox="0 0 16 16" fill="none">
          <path d="M2.5 8a5.5 5.5 0 0 1 9.4-3.9L13.5 2v4h-4l1.5-1.5A4 4 0 0 0 4 8M13.5 8a5.5 5.5 0 0 1-9.4 3.9L2.5 14v-4h4l-1.5 1.5A4 4 0 0 0 12 8"
            stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" fill="none" />
        </svg>
      );
    default:
      return null;
  }
}
