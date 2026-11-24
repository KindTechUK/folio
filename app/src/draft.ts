/**
 * Machine-draft disclaimer (liability control). It must appear in the UI and in
 * the header of every export, so these wrappers add it at the app boundary.
 */

import { exportJSON, exportText } from "@core/editor";
import type { EditorState } from "@core/editor";

/** The disclaimer, claiming speaker labels only when the transcript has them. */
export function draftDisclaimer(diarized: boolean): string {
  return diarized
    ? "MACHINE-GENERATED DRAFT — automatically transcribed and diarized. " +
        "Verify against the source audio before relying on it. Speaker labels and " +
        "wording may be inaccurate."
    : "MACHINE-GENERATED DRAFT — automatically transcribed. " +
        "Verify against the source audio before relying on it. Wording may be inaccurate.";
}

/** Text export with the disclaimer as a commented header block. */
export function exportTextWithDisclaimer(state: EditorState, diarized: boolean): string {
  const header = draftDisclaimer(diarized)
    .split(". ")
    .map((line) => `# ${line.trim()}`)
    .join("\n");
  return `${header}\n${exportText(state)}`;
}

/** JSON export with an added top-level `disclaimer` field. */
export function exportJSONWithDisclaimer(
  state: EditorState,
  diarized: boolean,
): { disclaimer: string } & ReturnType<typeof exportJSON> {
  return { disclaimer: draftDisclaimer(diarized), ...exportJSON(state) };
}
