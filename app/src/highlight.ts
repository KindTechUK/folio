/**
 * highlight — pure helpers for the playback word-highlight, free of the DOM.
 *
 * `<audio>` playback and the rAF highlight loop can only run in a real webview
 * (verify manually via `npm run tauri dev`); but the index-selection LOGIC is
 * pure and deterministic, so it is unit-tested here. The Editor's rAF loop calls
 * `activeWordIndex(starts, t)` once per frame against the flat word starts.
 */

import type { EditorState, EditorWord } from "@core/editor";

/**
 * Binary search for the index of the last word whose start <= t (the "active"
 * word at playback time t). Returns -1 if no word has started yet, or for an
 * empty list. Ported from the prototype's `activeAt`.
 *
 * `starts` MUST be non-decreasing (document order), which it always is for a
 * loaded transcript.
 */
export function activeWordIndex(starts: number[], t: number): number {
  let lo = 0;
  let hi = starts.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (starts[mid]! <= t) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}

/** All live words across all sentences, in document order. */
export function flatWords(state: EditorState): EditorWord[] {
  const out: EditorWord[] = [];
  for (const s of state.sentences) {
    for (const w of s.words) out.push(w);
  }
  return out;
}
