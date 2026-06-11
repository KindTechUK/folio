/**
 * draft — the machine-generated-draft disclaimer (liability control).
 *
 * Per the PRD, the disclaimer must appear in the UI AND in the header of every
 * export. Core's exporters stay pure/untouched; these thin wrappers add the
 * disclaimer at the app boundary: prepended to the text export, and as a
 * `disclaimer` field on the JSON export.
 */

import { exportJSON, exportText } from "@core/editor";
import type { EditorState } from "@core/editor";

/**
 * Shown persistently in the editor UI and stamped onto every export so a reader
 * is never handed a machine draft that looks authoritative without warning.
 */
export const DRAFT_DISCLAIMER =
  "MACHINE-GENERATED DRAFT — automatically transcribed and diarized. " +
  "Verify against the source audio before relying on it. Speaker labels and " +
  "wording may be inaccurate.";

/** Text export with the disclaimer as a commented header block. */
export function exportTextWithDisclaimer(state: EditorState): string {
  const header = DRAFT_DISCLAIMER.split(". ")
    .map((line) => `# ${line.trim()}`)
    .join("\n");
  return `${header}\n${exportText(state)}`;
}

/** JSON export with an added top-level `disclaimer` field. */
export function exportJSONWithDisclaimer(
  state: EditorState,
): { disclaimer: string } & ReturnType<typeof exportJSON> {
  return { disclaimer: DRAFT_DISCLAIMER, ...exportJSON(state) };
}
