import type { ReactNode } from "react";

// Small pill/badge; `tone` selects the colour pairing.

type Tone = "privacy" | "neutral" | "accent" | "danger";

export interface ChipProps {
  tone?: Tone;
  children: ReactNode;
  style?: React.CSSProperties;
}

const tones: Record<Tone, { bg: string; fg: string }> = {
  privacy: { bg: "var(--privacy-soft)", fg: "var(--privacy)" },
  neutral: { bg: "var(--slate-100)", fg: "var(--slate-600)" },
  accent: { bg: "var(--accent-soft)", fg: "var(--accent-strong)" },
  danger: { bg: "var(--danger-soft)", fg: "var(--danger)" },
};

export default function Chip({ tone = "neutral", children, style }: ChipProps) {
  const c = tones[tone];
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        background: c.bg,
        color: c.fg,
        borderRadius: 999,
        padding: "3px 10px",
        fontSize: "var(--text-xs)",
        fontWeight: 600,
        whiteSpace: "nowrap",
        ...style,
      }}
    >
      {children}
    </span>
  );
}
