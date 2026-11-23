import { Chip } from "./ui";

// "This computer": what Folio can do on this machine, in outcomes rather than
// specs. The tier is decided in Rust (`device.rs`); this file only words it.

/** Mirrors the Rust `DeviceProfile` returned by `device_profile`. */
export type DeviceProfile = {
  tier: "essentials" | "standard" | "copilot_plus" | "unknown";
  reason: string;
  ramGb: number | null;
  cpu: string | null;
  cores: number | null;
  npu: string | null;
  /** Minutes of work per hour of audio; null when Folio can't estimate. */
  minutesPerHour: number | null;
  /** Whether `minutesPerHour` was timed on this computer. */
  measured: boolean;
};

/** Shown when the check fails: never an error, just no estimates. */
export const UNCHECKED: DeviceProfile = {
  tier: "unknown",
  reason: "",
  ramGb: null,
  cpu: null,
  cores: null,
  npu: null,
  minutesPerHour: null,
  measured: false,
};

type TierCopy = {
  label: string;
  tone: "neutral" | "accent" | "privacy";
  lead: string;
  record: [string, string];
  notes: [string, string];
  notesAvailable: boolean;
};

const TIERS: Record<Exclude<DeviceProfile["tier"], "unknown">, TierCopy> = {
  essentials: {
    label: "Essentials",
    tone: "neutral",
    lead: "Folio works well on this computer for transcribing and recording.",
    record: [
      "Fine for meetings up to about 2 hours",
      "If the computer slows down, close apps you aren't using, such as Teams or a browser.",
    ],
    notes: [
      "Not on this computer",
      "Drafting notes will need 16 GB of memory. You can keep writing your own notes while recording.",
    ],
    notesAvailable: false,
  },
  standard: {
    label: "Standard",
    tone: "accent",
    lead: "Folio runs everything it offers on this computer.",
    record: ["Fine for long meetings", "You can keep Teams or Zoom open while recording."],
    notes: [
      "Yes, in a few minutes",
      "A 1-hour meeting will take a few minutes to draft, on this computer.",
    ],
    notesAvailable: true,
  },
  copilot_plus: {
    label: "Copilot+",
    tone: "privacy",
    lead: "Folio runs everything here, and notes will draft faster.",
    record: ["Fine for long meetings", "You can keep Teams or Zoom open while recording."],
    notes: [
      "Yes, in about a minute",
      "Uses the language model built into Windows, on this computer.",
    ],
    notesAvailable: true,
  },
};

/** "about 12 minutes" for `audioS` seconds of audio at `minutesPerHour`. */
export function aboutMinutes(audioS: number, minutesPerHour: number): string {
  const minutes = Math.max(1, Math.round((audioS / 3600) * minutesPerHour));
  return `about ${minutes} minute${minutes === 1 ? "" : "s"}`;
}

function npuNote(device: DeviceProfile): string {
  if (!device.npu) return "Not needed for transcription";
  return device.tier === "copilot_plus"
    ? "40 TOPS or more"
    : "Below the 40 TOPS that Copilot+ features need";
}

export function ThisComputer({ device }: { device: DeviceProfile }) {
  if (device.tier === "unknown") {
    return (
      <section className="computer-card" aria-labelledby="computer-title">
        <div className="computer-head">
          <span className="eyebrow" id="computer-title">
            This computer
          </span>
          <Chip tone="neutral">Not checked</Chip>
        </div>
        <p className="computer-lead">
          Folio couldn't check this computer, so it can't estimate how long things take.
        </p>
        <p className="estimate-note">
          Everything still works. Folio checks again each time it starts.
        </p>
      </section>
    );
  }

  const copy = TIERS[device.tier];
  return (
    <section className="computer-card" aria-labelledby="computer-title">
      <div className="computer-head">
        <span className="eyebrow" id="computer-title">
          This computer
        </span>
        <Chip tone={copy.tone}>{copy.label}</Chip>
      </div>
      <p className="computer-lead">{copy.lead}</p>
      <div className="computer-tiles">
        <div className="computer-tile">
          <span className="what">Transcribing a 1-hour recording</span>
          <span className="answer tabular">
            {device.minutesPerHour === null ? "Not estimated" : aboutMinutes(3600, device.minutesPerHour)}
          </span>
          <p>
            {device.measured
              ? "Measured from your recent transcripts."
              : "Folio refines this after each transcript you make."}
          </p>
        </div>
        <div className="computer-tile">
          <span className="what">Recording a meeting</span>
          <span className="answer">{copy.record[0]}</span>
          <p>{copy.record[1]}</p>
        </div>
        <div className="computer-tile">
          <span className="what">
            Drafting notes from a transcript <span className="later">Coming</span>
          </span>
          <span className={copy.notesAvailable ? "answer" : "answer muted"}>{copy.notes[0]}</span>
          <p>{copy.notes[1]}</p>
        </div>
      </div>
      {device.tier === "essentials" && (
        <div className="computer-advice">
          <strong>Choosing a new laptop?</strong> Ask for 16 GB of memory. That's what matters most
          for Folio. An “AI PC” sticker isn't needed.
        </div>
      )}
      <details className="computer-details">
        <summary>Technical details</summary>
        <dl className="computer-specs">
          <div>
            <dt>Memory</dt>
            <dd className="tabular">
              {device.ramGb === null ? "Unknown" : `${device.ramGb.toFixed(1)} GB usable`}
            </dd>
          </div>
          <div>
            <dt>Processor</dt>
            <dd>
              {device.cpu ?? "Unknown"}
              {device.cores !== null && <small>{device.cores} cores</small>}
            </dd>
          </div>
          <div>
            <dt>AI accelerator (NPU)</dt>
            <dd>
              {device.npu ?? "None"}
              <small>{npuNote(device)}</small>
            </dd>
          </div>
        </dl>
        <p>
          {copy.label}: {device.reason} Folio sorts computers into three groups when it starts:{" "}
          <strong>Essentials</strong> (under 12 GB of memory), <strong>Standard</strong> (12 GB or
          more) and <strong>Copilot+</strong> (16 GB or more with an AI accelerator of about 40
          TOPS). The check runs on this computer and nothing is sent anywhere.
        </p>
      </details>
    </section>
  );
}

/** Record page: on Essentials only, a dismissable tip before a long meeting. */
export function RecordHint({
  device,
  onDismiss,
}: {
  device: DeviceProfile;
  onDismiss: () => void;
}) {
  if (device.tier !== "essentials") return null;
  const gb = device.ramGb === null ? "limited" : `${Math.round(device.ramGb)} GB of`;
  return (
    <div className="computer-hint" role="note">
      <span className="computer-hint-icon" aria-hidden="true">
        i
      </span>
      <span>
        This computer has {gb} memory. For meetings over an hour, close apps you don't need
        before you start.{" "}
        <button type="button" onClick={onDismiss}>
          Got it
        </button>
      </span>
    </div>
  );
}

/** Import page: how long the chosen recording will take here. */
export function ImportEstimate({
  device,
  durationS,
  labelSpeakers,
}: {
  device: DeviceProfile;
  /** Length of the chosen recording; null when its file doesn't say. */
  durationS: number | null;
  labelSpeakers: boolean;
}) {
  const perHour = device.minutesPerHour;
  if (perHour === null) {
    return (
      <p className="import-estimate" data-testid="import-estimate">
        Ready to transcribe. Folio can't estimate the time on this computer.
      </p>
    );
  }
  const speakers = labelSpeakers ? ", plus time to label speakers" : "";
  if (durationS === null) {
    return (
      <p className="import-estimate" data-testid="import-estimate">
        Ready to transcribe: <strong>{aboutMinutes(3600, perHour)}</strong> for each hour of
        audio on this computer{speakers}.
      </p>
    );
  }
  return (
    <p className="import-estimate" data-testid="import-estimate">
      Ready to transcribe: <strong className="tabular">{aboutMinutes(durationS, perHour)}</strong>{" "}
      on this computer{speakers}.
    </p>
  );
}
