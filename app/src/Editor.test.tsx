import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { useState } from "react";
import Editor from "./Editor";
import { loadTranscript } from "@core/editor";
import type { EditorState } from "@core/editor";
import type { Transcript } from "@core/types";

// Two speakers, two turns. SPEAKER_00 says one sentence, SPEAKER_01 another.
const FIXTURE: Transcript = {
  segments: [
    {
      start: 0,
      end: 2,
      text: "The witness arrived.",
      speaker: "SPEAKER_00",
      words: [
        { word: "The", start: 0, end: 0.5, speaker: "SPEAKER_00" },
        { word: "witness", start: 0.5, end: 1.2, speaker: "SPEAKER_00" },
        { word: "arrived.", start: 1.2, end: 2, speaker: "SPEAKER_00" },
      ],
    },
    {
      start: 2,
      end: 4,
      text: "I object loudly.",
      speaker: "SPEAKER_01",
      words: [
        { word: "I", start: 2, end: 2.3, speaker: "SPEAKER_01" },
        { word: "object", start: 2.3, end: 3.1, speaker: "SPEAKER_01" },
        { word: "loudly.", start: 3.1, end: 4, speaker: "SPEAKER_01" },
      ],
    },
  ],
};

/** Test harness owning EditorState, exposing it via a ref-like callback. */
function Harness({
  onState,
  diarized = true,
  transcript = FIXTURE,
  audioSrc,
}: {
  onState: (s: EditorState) => void;
  diarized?: boolean;
  transcript?: Transcript;
  audioSrc?: string;
}) {
  const [state, setState] = useState<EditorState>(() =>
    loadTranscript(transcript),
  );
  onState(state);
  return (
    <Editor
      state={state}
      diarized={diarized}
      audioSrc={audioSrc}
      setState={(s) => {
        onState(s);
        setState(s);
      }}
    />
  );
}

describe("Editor (jsdom)", () => {
  let latest: EditorState;
  const capture = (s: EditorState) => {
    latest = s;
  };

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("double-click a sentence, edit text, blur -> words and DOM change", () => {
    render(<Harness onState={capture} />);

    const sentence = screen.getByTestId("sentence-sent-0");
    fireEvent.doubleClick(sentence);

    const ta = screen.getByTestId("edit-sent-0") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "the deponent showed up here." } });
    fireEvent.blur(ta);

    const edited = latest.sentences.find((s) => s.id === "sent-0")!;
    const text = edited.words.map((w) => w.text).join(" ");
    expect(text).toBe("the deponent showed up here.");
    expect(screen.getByTestId("sentence-sent-0")).toHaveTextContent("deponent");
  });

  it("click a speaker chip -> inline rename -> ALL that speaker's chips show new name", () => {
    // Build a fixture where the same speaker appears in two non-adjacent turns.
    const t: Transcript = {
      segments: [
        ...FIXTURE.segments,
        {
          start: 4,
          end: 5,
          text: "And again.",
          speaker: "SPEAKER_00",
          words: [
            { word: "And", start: 4, end: 4.4, speaker: "SPEAKER_00" },
            { word: "again.", start: 4.4, end: 5, speaker: "SPEAKER_00" },
          ],
        },
      ],
    };
    function H() {
      const [state, setState] = useState<EditorState>(() => loadTranscript(t));
      capture(state);
      return (
        <Editor
          state={state}
          diarized
          setState={(s) => {
            capture(s);
            setState(s);
          }}
        />
      );
    }
    render(<H />);

    const chips = screen
      .getAllByTestId("speaker-chip")
      .filter((c) => c.getAttribute("data-speaker") === "SPEAKER_00");
    expect(chips.length).toBe(2); // two SPEAKER_00 turns

    // Clicking the chip swaps it for a seeded inline input.
    fireEvent.click(chips[0]!);
    const input = screen.getByTestId("speaker-rename-input") as HTMLInputElement;
    expect(input.value).toBe("SPEAKER_00"); // seeded with current display name
    fireEvent.change(input, { target: { value: "Counsel" } });
    fireEvent.keyDown(input, { key: "Enter" });

    const renamed = screen
      .getAllByTestId("speaker-chip")
      .filter((c) => c.getAttribute("data-speaker") === "SPEAKER_00");
    for (const c of renamed) expect(c).toHaveTextContent("Counsel");
    // the other speaker is untouched
    const other = screen
      .getAllByTestId("speaker-chip")
      .filter((c) => c.getAttribute("data-speaker") === "SPEAKER_01");
    for (const c of other) expect(c).toHaveTextContent("SPEAKER_01");
  });

  it("reassign one sentence -> only that sentence's speaker changes", () => {
    render(<Harness onState={capture} />);
    const before = latest.sentences.map((s) => s.words[0]!.speaker);
    expect(before).toEqual(["SPEAKER_00", "SPEAKER_01"]);

    // The first turn's reassign select targets sent-0; reassign it to SPEAKER_01.
    const select = screen.getByTestId("reassign-sent-0") as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "SPEAKER_01" } });

    expect(latest.sentences.find((s) => s.id === "sent-0")!.words.every(
      (w) => w.speaker === "SPEAKER_01",
    )).toBe(true);
    // sent-1 unchanged
    expect(latest.sentences.find((s) => s.id === "sent-1")!.words[0]!.speaker).toBe(
      "SPEAKER_01",
    );
  });

  it("renders the persistent draft disclaimer banner", () => {
    render(<Harness onState={capture} />);
    expect(
      screen.getByLabelText("machine-generated draft disclaimer"),
    ).toHaveTextContent("MACHINE-GENERATED DRAFT — automatically transcribed and diarized");
  });

  it("diarized=false renders paragraphs without speaker controls; diarized=true renders chips", () => {
    const { unmount } = render(<Harness onState={capture} diarized={false} />);

    // Paragraph blocks render, and there are NO speaker chips / reassign selects.
    expect(screen.getAllByTestId("paragraph").length).toBeGreaterThan(0);
    expect(screen.queryByTestId("speaker-chip")).toBeNull();
    expect(screen.queryByTestId("reassign-sent-0")).toBeNull();

    // Words still render (for karaoke + click-to-seek).
    expect(screen.getAllByTestId("word").length).toBeGreaterThan(0);
    unmount();

    render(<Harness onState={capture} diarized />);
    expect(screen.getAllByTestId("speaker-chip").length).toBeGreaterThan(0);
    expect(screen.queryByTestId("paragraph")).toBeNull();
  });

  it("paragraph mode: double-click a paragraph -> edit whole text -> commit collapses the run", () => {
    render(<Harness onState={capture} diarized={false} />);
    // FIXTURE's two sentences (no gap) group into one paragraph keyed on sent-0.

    const para = screen.getAllByTestId("paragraph")[0]!;
    // The whole paragraph is one editable surface seeded with its full text.
    expect(para).toHaveTextContent("The witness arrived.");
    expect(para).toHaveTextContent("I object loudly.");

    fireEvent.doubleClick(para);
    const ta = screen.getByTestId("edit-sent-0") as HTMLTextAreaElement;
    // Seeded with the joined paragraph text.
    expect(ta.value).toBe("The witness arrived. I object loudly.");
    fireEvent.change(ta, {
      target: { value: "the deponent calmly entered and then objected." },
    });
    fireEvent.blur(ta);

    // Whole run collapsed to one sentence (id = first sentence's id).
    const edited = latest.sentences.find((s) => s.id === "sent-0")!;
    expect(edited.words.map((w) => w.text).join(" ")).toBe(
      "the deponent calmly entered and then objected.",
    );
    // The second sentence is gone (collapsed into sent-0).
    expect(latest.sentences.find((s) => s.id === "sent-1")).toBeUndefined();

    // DOM reflects the edit.
    expect(screen.getAllByTestId("paragraph")[0]!).toHaveTextContent("deponent");
  });

  it("find & replace: open, count matches across sentences, replace all", () => {
    // "I" appears once; use a term present in >= 2 sentences. Build a fixture
    // where "the" appears in both sentences.
    const t: Transcript = {
      segments: [
        {
          start: 0,
          end: 2,
          text: "the witness arrived.",
          speaker: "SPEAKER_00",
          words: [
            { word: "the", start: 0, end: 0.5, speaker: "SPEAKER_00" },
            { word: "witness", start: 0.5, end: 1.2, speaker: "SPEAKER_00" },
            { word: "arrived.", start: 1.2, end: 2, speaker: "SPEAKER_00" },
          ],
        },
        {
          start: 2,
          end: 4,
          text: "the clerk left.",
          speaker: "SPEAKER_01",
          words: [
            { word: "the", start: 2, end: 2.3, speaker: "SPEAKER_01" },
            { word: "clerk", start: 2.3, end: 3.1, speaker: "SPEAKER_01" },
            { word: "left.", start: 3.1, end: 4, speaker: "SPEAKER_01" },
          ],
        },
      ],
    };
    render(<Harness onState={capture} diarized transcript={t} />);

    // Open the find & replace row.
    fireEvent.click(screen.getByTestId("toggle-find"));
    const find = screen.getByTestId("find-input");
    const replace = screen.getByTestId("replace-input");
    fireEvent.change(find, { target: { value: "the" } });
    fireEvent.change(replace, { target: { value: "a" } });

    // Live count reflects both sentences (case-insensitive by default).
    expect(screen.getByTestId("find-count")).toHaveTextContent("2 matches");

    fireEvent.click(screen.getByTestId("replace-all"));

    // Rendered words changed across both sentences; captured state reflects it.
    const joined = latest.sentences
      .map((s) => s.words.map((w) => w.text).join(" "))
      .join(" | ");
    expect(joined).toBe("a witness arrived. | a clerk left.");
    expect(latest.sentences.every((s) => s.words[0]!.text === "a")).toBe(true);
  });

  it("undo button: disabled when canUndo=false, calls onUndo when enabled", () => {
    const onUndo = vi.fn();
    function H({ can }: { can: boolean }) {
      const [state, setState] = useState<EditorState>(() =>
        loadTranscript(FIXTURE),
      );
      return (
        <Editor
          state={state}
          diarized
          setState={setState}
          onUndo={onUndo}
          canUndo={can}
        />
      );
    }
    const { rerender } = render(<H can={false} />);
    expect(screen.getByTestId("undo")).toBeDisabled();

    rerender(<H can={true} />);
    fireEvent.click(screen.getByTestId("undo"));
    expect(onUndo).toHaveBeenCalled();
  });

  it.each([
    ["sentence", true, "sentence-sent-0"],
    ["paragraph", false, "paragraph"],
  ])("clicking inside an open %s editor does not toggle playback", (_, diarized, target) => {
    const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    const { container } = render(
      <Harness onState={capture} diarized={diarized as boolean} audioSrc="clip.wav" />,
    );

    fireEvent.doubleClick(screen.getAllByTestId(target as string)[0]!);
    fireEvent.click(screen.getByTestId("edit-sent-0"));
    expect(play).not.toHaveBeenCalled();

    fireEvent.click(container.querySelector(".doc")!);
    expect(play).toHaveBeenCalledTimes(1);
  });

  // NOTE: rAF karaoke highlight, audio play/pause, Space play/pause, click-to-
  // pause, and the Follow auto-scroll are NOT jsdom-testable (no real <audio> /
  // layout / scroll). They are verified manually via `npm run tauri dev`. Undo/
  // redo stack correctness is covered by history.test.ts.
});
