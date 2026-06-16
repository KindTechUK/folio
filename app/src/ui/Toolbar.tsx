import type { ReactNode } from "react";

// A horizontal control container that groups its children with vertical
// dividers between groups. Each <ToolbarGroup> is one logical cluster (e.g.
// playback, edit, export) so the editor transport reads as grouped tools rather
// than one undifferentiated row of buttons.

export function Toolbar({
  children,
  sticky = false,
  style,
}: {
  children: ReactNode;
  sticky?: boolean;
  style?: React.CSSProperties;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        flexWrap: "wrap",
        gap: "var(--space-3)",
        padding: "8px 0",
        ...(sticky
          ? {
              position: "sticky",
              top: 0,
              background: "var(--surface)",
              borderBottom: "1px solid var(--slate-200)",
              zIndex: 5,
            }
          : null),
        ...style,
      }}
    >
      {children}
    </div>
  );
}

export function ToolbarGroup({
  children,
  divider = true,
}: {
  children: ReactNode;
  divider?: boolean;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-2)",
        ...(divider
          ? {
              borderRight: "1px solid var(--slate-200)",
              paddingRight: "var(--space-3)",
            }
          : null),
      }}
    >
      {children}
    </div>
  );
}
