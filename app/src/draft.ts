/**
 * Machine-draft disclaimer (liability control). It must appear in the UI and in
 * the header of every export, so these wrappers add it at the app boundary.
 */

import { exportJSON, exportText } from "@core/editor";
import type { EditorState } from "@core/editor";

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
