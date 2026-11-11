/** Speaker labelling for a recording: a switch, a count, and a plain summary of what will happen. */

const MAX_SPEAKERS = 20;

/** What Stop will do about speakers, in words. */
export function speakerSummary(labelSpeakers: boolean, speakerCount: string): string {
  if (!labelSpeakers) return "Speakers won't be labelled.";
  const n = parseInt(speakerCount, 10);
  if (!Number.isFinite(n) || n < 1) {
    return "Speakers will be labelled when you stop. Folio will work out how many.";
  }
  return `Speakers will be labelled when you stop, assuming ${n} ${n === 1 ? "person" : "people"}.`;
}

export default function SpeakerBar({
  labelSpeakers,
  onLabelSpeakers,
  speakerCount,
  onSpeakerCount,
  disabled = false,
}: {
  labelSpeakers: boolean;
  onLabelSpeakers: (on: boolean) => void;
  /** "" means work it out automatically. */
  speakerCount: string;
  onSpeakerCount: (count: string) => void;
  disabled?: boolean;
}) {
  const n = parseInt(speakerCount, 10);
  const count = Number.isFinite(n) && n > 0 ? n : 0;
  // Below 1 the count goes back to automatic; up from automatic starts at 2.
  const fewer = () => onSpeakerCount(count <= 1 ? "" : String(count - 1));
  const more = () => onSpeakerCount(String(count === 0 ? 2 : Math.min(count + 1, MAX_SPEAKERS)));

  return (
    <div className={labelSpeakers ? "speaker-bar on" : "speaker-bar"} role="group" aria-label="Speaker labels">
      <p className="speaker-summary" data-testid="speaker-summary" aria-live="polite">
        {speakerSummary(labelSpeakers, speakerCount)}
      </p>
      <div className="speaker-controls">
        {labelSpeakers && (
          <div className="speaker-stepper">
            <span id="speaker-count-label">Speakers</span>
            <button
              type="button"
              onClick={fewer}
              disabled={disabled || count === 0}
              aria-label="Fewer speakers"
            >
              −
            </button>
            <output
              className="tabular"
              aria-labelledby="speaker-count-label"
              data-testid="speaker-count-value"
            >
              {count === 0 ? "Auto" : count}
            </output>
            <button
              type="button"
              onClick={more}
              disabled={disabled || count >= MAX_SPEAKERS}
              aria-label="More speakers"
            >
              +
            </button>
          </div>
        )}
        <label className="speaker-switch">
          <span>Label speakers</span>
          <input
            type="checkbox"
            role="switch"
            data-testid="label-speakers"
            checked={labelSpeakers}
            disabled={disabled}
            onChange={(e) => onLabelSpeakers(e.target.checked)}
          />
        </label>
      </div>
    </div>
  );
}
