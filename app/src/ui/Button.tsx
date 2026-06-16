import type { ButtonHTMLAttributes } from "react";

// Presentational button primitive. Variants map to the token palette; styling is
// driven by a tiny inline style table (no CSS-module pipeline in this app), but
// every value comes from the design tokens in theme.css via CSS variables.

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

const base: React.CSSProperties = {
  font: "inherit",
  fontWeight: 600,
  lineHeight: 1.2,
  borderRadius: "var(--radius-sm)",
  border: "1px solid transparent",
  cursor: "pointer",
  whiteSpace: "nowrap",
  transition: "background 120ms ease, border-color 120ms ease",
};

const sizes: Record<Size, React.CSSProperties> = {
  sm: { fontSize: "var(--text-xs)", padding: "5px 10px" },
  md: { fontSize: "var(--text-sm)", padding: "8px 14px" },
};

const variants: Record<Variant, React.CSSProperties> = {
  primary: {
    background: "var(--accent)",
    color: "var(--accent-contrast)",
    borderColor: "var(--accent)",
  },
  secondary: {
    background: "var(--surface)",
    color: "var(--slate-700)",
    borderColor: "var(--slate-300)",
  },
  ghost: {
    background: "transparent",
    color: "var(--slate-600)",
    borderColor: "transparent",
  },
  danger: {
    background: "var(--danger-soft)",
    color: "var(--danger)",
    borderColor: "var(--danger-border)",
  },
};

export default function Button({
  variant = "secondary",
  size = "md",
  style,
  disabled,
  ...rest
}: ButtonProps) {
  return (
    <button
      disabled={disabled}
      style={{
        ...base,
        ...sizes[size],
        ...variants[variant],
        ...(disabled ? { opacity: 0.5, cursor: "not-allowed" } : null),
        ...style,
      }}
      {...rest}
    />
  );
}
