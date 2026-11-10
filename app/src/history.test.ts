import { describe, it, expect } from "vitest";
import {
  initHistory,
  pushHistory,
  undo,
  redo,
  canUndo,
  canRedo,
} from "./history";

describe("initHistory", () => {

  it("cannot undo or redo a fresh history", () => {
    const h = initHistory("a");
    expect(canUndo(h)).toBe(false);
    expect(canRedo(h)).toBe(false);
  });
});

describe("pushHistory", () => {
  it("makes undo available after a push", () => {
    const h = pushHistory(initHistory("a"), "b");
    expect(h.present).toBe("b");
    expect(canUndo(h)).toBe(true);
    expect(canRedo(h)).toBe(false);
  });

  it("clears future so a new edit discards the redo branch", () => {
    // a -> b -> c, undo back to b (future now holds c), then push d.
    let h = pushHistory(initHistory("a"), "b");
    h = pushHistory(h, "c");
    h = undo(h);
    expect(h.present).toBe("b");
    expect(canRedo(h)).toBe(true);

    h = pushHistory(h, "d");
    expect(h.present).toBe("d");
    expect(canRedo(h)).toBe(false);
    // redo is now a no-op (nothing on the redo branch).
    expect(redo(h)).toEqual(h);
  });

});

describe("undo", () => {
  it("walks back through pushed states in LIFO order", () => {
    let h = pushHistory(initHistory("a"), "b");
    h = pushHistory(h, "c");
    expect(h.present).toBe("c");

    h = undo(h);
    expect(h.present).toBe("b");
    expect(canUndo(h)).toBe(true);

    h = undo(h);
    expect(h.present).toBe("a");
    expect(canUndo(h)).toBe(false);
  });

  it("is a no-op when there is nothing to undo", () => {
    const h = initHistory("a");
    expect(undo(h)).toEqual(h);
    expect(undo(h).present).toBe("a");
  });

});

describe("redo", () => {
  it("restores forward after an undo", () => {
    let h = pushHistory(initHistory("a"), "b");
    h = pushHistory(h, "c");

    h = undo(h);
    h = undo(h);
    expect(h.present).toBe("a");
    expect(canRedo(h)).toBe(true);

    h = redo(h);
    expect(h.present).toBe("b");
    expect(canRedo(h)).toBe(true);

    h = redo(h);
    expect(h.present).toBe("c");
    expect(canRedo(h)).toBe(false);
  });

  it("is a no-op when there is nothing to redo", () => {
    const h = pushHistory(initHistory("a"), "b");
    expect(canRedo(h)).toBe(false);
    expect(redo(h)).toEqual(h);
    expect(redo(h).present).toBe("b");
  });

});

describe("immutability", () => {
  it("leaves the input history unchanged after push, undo and redo", () => {
    const h = pushHistory(initHistory("a"), "b");
    pushHistory(h, "c");
    redo(undo(h));
    expect(h.present).toBe("b");
    expect(canUndo(h)).toBe(true);
    expect(canRedo(h)).toBe(false);
    expect(undo(h).present).toBe("a");
  });
});
