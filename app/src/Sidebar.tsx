import { Chip } from "./ui";

// The app shell's left rail: serif wordmark, primary navigation, and a pinned
// footer privacy chip. The privacy signal lives here — calm and always visible —
// rather than as a loud banner competing with genuine warnings.

export type View = "library" | "transcribe" | "models";

const NAV: { id: View; label: string; hint: string }[] = [
  { id: "library", label: "Library", hint: "Saved matters" },
  { id: "transcribe", label: "New transcription", hint: "Transcribe audio" },
  { id: "models", label: "Models", hint: "Signed model packs" },
];

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
        width: 232,
        flexShrink: 0,
        background: "var(--surface)",
        borderRight: "1px solid var(--slate-200)",
        display: "flex",
        flexDirection: "column",
        padding: "var(--space-5) var(--space-4)",
        height: "100vh",
        position: "sticky",
        top: 0,
      }}
    >
      <div style={{ marginBottom: "var(--space-6)" }}>
        <div
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: "var(--text-xl)",
            fontWeight: 600,
            color: "var(--ink)",
            lineHeight: 1.15,
          }}
        >
          Folio
        </div>
        <div
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: "var(--text-lg)",
            color: "var(--slate-500)",
            lineHeight: 1.15,
          }}
        >
          Private transcription
        </div>
      </div>

      <nav style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {NAV.map((item) => {
          const active = view === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.id)}
              aria-pressed={active}
              style={{
                textAlign: "left",
                font: "inherit",
                cursor: "pointer",
                border: "none",
                borderRadius: "var(--radius)",
                padding: "9px 12px",
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
        })}
      </nav>

      <div style={{ marginTop: "auto", paddingTop: "var(--space-5)" }}>
        <Chip tone="privacy" style={{ width: "100%", justifyContent: "center" }}>
          🔒 Local only · airplane-mode
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
