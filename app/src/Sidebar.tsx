import { Chip } from "./ui";

export type View = "library" | "transcribe" | "models";

const PRIMARY_NAV: { id: View; label: string; hint: string }[] = [
  { id: "library", label: "Transcripts", hint: "Saved drafts" },
  { id: "transcribe", label: "Import recording", hint: "Start a local transcript" },
];

const SETTINGS_NAV: { id: View; label: string; hint: string }[] = [
  { id: "models", label: "Models & settings", hint: "Verified model packs" },
];

function NavigationItem({
  item,
  active,
  onNavigate,
}: {
  item: { id: View; label: string; hint: string };
  active: boolean;
  onNavigate: (view: View) => void;
}) {
  return (
    <button
      onClick={() => onNavigate(item.id)}
      aria-current={active ? "page" : undefined}
      style={{
        textAlign: "left",
        font: "inherit",
        cursor: "pointer",
        border: "1px solid transparent",
        borderRadius: "var(--radius-sm)",
        padding: "8px 10px",
        background: active ? "var(--accent-soft)" : "transparent",
        color: active ? "var(--accent-strong)" : "var(--slate-600)",
        fontWeight: active ? 600 : 500,
        display: "flex",
        flexDirection: "column",
        gap: 1,
      }}
    >
      <span>{item.label}</span>
      <span style={{ fontSize: "var(--text-xs)", color: "var(--slate-400)" }}>
        {item.hint}
      </span>
    </button>
  );
}

export default function Sidebar({
  view,
  onNavigate,
}: {
  view: View;
  onNavigate: (v: View) => void;
}) {
  return (
    <aside
      style={{
        width: 208,
        flexShrink: 0,
        background: "var(--surface)",
        borderRight: "1px solid var(--slate-200)",
        display: "flex",
        flexDirection: "column",
        padding: "20px 14px",
        height: "100vh",
        position: "sticky",
        top: 0,
      }}
    >
      <div style={{ marginBottom: "var(--space-5)" }}>
        <div
          style={{
            fontSize: "var(--text-xl)",
            fontWeight: 650,
            color: "var(--ink)",
            lineHeight: 1.15,
          }}
        >
          Folio
        </div>
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--slate-500)",
            lineHeight: 1.4,
            marginTop: 3,
          }}
        >
          Private transcription
        </div>
      </div>

      <nav aria-label="Main navigation" style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {PRIMARY_NAV.map((item) => (
          <NavigationItem
            key={item.id}
            item={item}
            active={view === item.id}
            onNavigate={onNavigate}
          />
        ))}
      </nav>

      <div
        style={{
          marginTop: "var(--space-5)",
          paddingTop: "var(--space-4)",
          borderTop: "1px solid var(--slate-200)",
        }}
      >
        <div
          style={{
            padding: "0 10px var(--space-2)",
            color: "var(--slate-400)",
            fontSize: "var(--text-xs)",
            fontWeight: 600,
          }}
        >
          Administration
        </div>
        {SETTINGS_NAV.map((item) => (
          <NavigationItem
            key={item.id}
            item={item}
            active={view === item.id}
            onNavigate={onNavigate}
          />
        ))}
      </div>

      <div style={{ marginTop: "auto", paddingTop: "var(--space-5)" }}>
        <Chip tone="privacy" style={{ width: "100%", justifyContent: "center" }}>
          Local only · no network
        </Chip>
        <p
          style={{
            margin: "var(--space-2) 0 0",
            fontSize: "var(--text-xs)",
            color: "var(--slate-400)",
            lineHeight: 1.4,
          }}
        >
          No network access. Audio and transcripts never leave this device.
        </p>
        <p
          style={{
            margin: "var(--space-3) 0 0",
            fontSize: "var(--text-xs)",
            color: "var(--slate-400)",
          }}
        >
          © 2026 KindTech
        </p>
      </div>
    </aside>
  );
}
