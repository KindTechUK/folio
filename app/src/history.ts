// Pure, framework-free undo/redo stack.
//
// Models edit history as a past/present/future zipper: `past` holds prior
// states (oldest first), `present` is the current one, and `future` holds
// states reachable by redo (next-to-redo first). The editor in App.tsx wraps
// its `EditorState` in one of these so edits become undoable; this module
// stays React-free and generic over `T` so it is unit-testable under vitest
// like the other `app/src/*.ts` pure helpers.
//
// Every function is PURE: it never mutates the input `History` or its arrays,
// always returning fresh objects/arrays.

export interface History<T> {
  /** Prior states, oldest first; the last element is the most recent undo target. */
  past: T[];
  /** The current state. */
  present: T;
  /** Redoable states, next-to-redo first. */
  future: T[];
}

/** Create a history seeded with `present` and nothing to undo or redo. */
export function initHistory<T>(present: T): History<T> {
  return { past: [], present, future: [] };
}

/**
 * Record a new state. The old `present` is pushed onto `past`, `next` becomes
 * the new `present`, and `future` is CLEARED — a fresh edit after undoing
 * discards the redo branch.
 */
export function pushHistory<T>(h: History<T>, next: T): History<T> {
  return { past: [...h.past, h.present], present: next, future: [] };
}

/**
 * Step back one state: the current `present` moves to the front of `future`
 * and the most recent `past` entry becomes `present`. No-op (returns an
 * equal-valued history) when there is nothing to undo.
 */
export function undo<T>(h: History<T>): History<T> {
  if (h.past.length === 0) return h;
  const previous = h.past[h.past.length - 1];
  return {
    past: h.past.slice(0, -1),
    present: previous,
    future: [h.present, ...h.future],
  };
}

/**
 * Step forward one state: the first `future` entry becomes `present` and the
 * current `present` is pushed onto `past`. No-op (returns an equal-valued
 * history) when there is nothing to redo.
 */
export function redo<T>(h: History<T>): History<T> {
  if (h.future.length === 0) return h;
  const [next, ...rest] = h.future;
  return {
    past: [...h.past, h.present],
    present: next,
    future: rest,
  };
}

/** Whether there is a prior state to undo to. */
export function canUndo<T>(h: History<T>): boolean {
  return h.past.length > 0;
}

/** Whether there is a forward state to redo to. */
export function canRedo<T>(h: History<T>): boolean {
  return h.future.length > 0;
}
