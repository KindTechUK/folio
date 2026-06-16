import type { ReactNode } from "react";

// One banner component for every in-app notice. `privacy` is the calm trust
// signal, `warning` is reserved for genuine warnings (e.g. unencrypted volume),
// `info` is for quiet advisories (e.g. the machine-draft disclaimer).

type Variant = "privacy" | "warning" | "info";

export interface BannerProps {
  variant?: Variant;
  children: ReactNode;
  onDismiss?: () => void;
  /** Forwarded to the root element (e.g. for data-testid / role). */
  role?: string;
  "data-testid"?: string;
  "aria-label"?: string;
  dismissLabel?: string;
  dismissTestid?: string;
  style?: React.CSSProperties;
}

const palette: Record<
  Variant,
  { bg: string; border: string; fg: string }
> = {
  privacy: {
    bg: "var(--privacy-soft)",
    border: "var(--privacy)",
    fg: "var(--privacy)",
  },
  warning: {
    bg: "var(--warning-soft)",
    border: "var(--warning-border)",
    fg: "var(--warning)",
  },
  info: { bg: "var(--info-soft)", border: "var(--slate-300)", fg: "var(--slate-700)" },
};

export default function Banner({
  variant = "info",
  children,
  onDismiss,
  role,
  "data-testid": testId,
  "aria-label": ariaLabel,
  dismissLabel = "dismiss",
  dismissTestid,
  style,
}: BannerProps) {
  const c = palette[variant];
  return (
    <div
      role={role}
      data-testid={testId}
      aria-label={ariaLabel}
      style={{
        display: "flex",
        gap: "var(--space-3)",
        alignItems: "flex-start",
        background: c.bg,
        border: `1px solid ${c.border}`,
        color: c.fg,
        padding: "10px 14px",
        borderRadius: "var(--radius)",
        fontSize: "var(--text-sm)",
        lineHeight: 1.45,
        ...style,
      }}
    >
      <span style={{ flex: 1 }}>{children}</span>
      {onDismiss && (
        <button
          aria-label={dismissLabel}
          data-testid={dismissTestid}
          onClick={onDismiss}
          style={{
            background: "none",
            border: "none",
            color: c.fg,
            cursor: "pointer",
            fontSize: 18,
            lineHeight: 1,
            padding: 0,
          }}
        >
          ×
        </button>
      )}
    </div>
  );
}
