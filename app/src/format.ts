import type { EditorState } from "@core/editor";

/** Seconds as m:ss (negative clamps to 0:00). */
export function mmss(t: number): string {
  const s = Math.max(0, t);
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

/** A speaker's renamed label, or its positional id if not renamed. */
export function displayName(state: EditorState, id: string): string {
  return state.renameMap[id] ?? id;
}
