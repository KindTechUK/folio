/** "Transcript options": opt-in speaker detection with an optional count. */
export default function SpeakerOptions({
  detectSpeakers,
  onDetectSpeakers,
  speakerCount,
  onSpeakerCount,
  disabled = false,
}: {
  detectSpeakers: boolean;
  onDetectSpeakers: (on: boolean) => void;
  speakerCount: string;
  onSpeakerCount: (count: string) => void;
  disabled?: boolean;
}) {
  return (
    <div
      style={{
        paddingTop: "var(--space-4)",
        borderTop: "1px solid var(--slate-200)",
        fontSize: "var(--text-sm)",
        color: "var(--slate-600)",
      }}
    >
      <div style={{ fontWeight: 600, marginBottom: "var(--space-2)" }}>
        Transcript options
      </div>
      <label
        style={{
          display: "flex",
          gap: 8,
          alignItems: "center",
          cursor: "pointer",
        }}
        title="Detect and label distinct speakers. Slower; best on clear 1–2 speaker audio."
      >
        <input
          type="checkbox"
          data-testid="detect-speakers"
          checked={detectSpeakers}
          disabled={disabled}
          onChange={(e) => onDetectSpeakers(e.target.checked)}
        />
        Detect and label speakers (slower)
      </label>
      {detectSpeakers && (
        <label
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            margin: "var(--space-3) 0 0 24px",
          }}
        >
          Expected number of speakers
          <input
            type="number"
            min={1}
            data-testid="speaker-count"
            placeholder="Auto"
            value={speakerCount}
            disabled={disabled}
            onChange={(e) => onSpeakerCount(e.target.value)}
            style={{ width: 72 }}
          />
        </label>
      )}
    </div>
  );
}

/** The speaker count to request, or undefined for auto / no detection. */
export function requestedSpeakers(detectSpeakers: boolean, speakerCount: string): number | undefined {
  const n = parseInt(speakerCount, 10);
  return detectSpeakers && Number.isFinite(n) && n > 0 ? n : undefined;
}
