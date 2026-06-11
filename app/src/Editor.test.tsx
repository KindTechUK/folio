import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { useState } from "react";
import Editor from "./Editor";
import { loadTranscript } from "@core/editor";
import type { EditorState } from "@core/editor";
import type { Transcript } from "@core/types";
import {
  exportTextWithDisclaimer,
  exportJSONWithDisclaimer,
  DRAFT_DISCLAIMER,
} from "./draft";

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
}: {
  onState: (s: EditorState) => void;
  diarized?: boolean;
  transcript?: Transcript;
}) {
  const [state, setState] = useState<EditorState>(() =>
    loadTranscript(transcript),
  );
  onState(state);
  return (
    <Editor
      state={state}
      diarized={diarized}
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

  it("double-click a sentence, edit text, blur -> words change, timings stay in span", () => {
    render(<Harness onState={capture} />);
    const span = { start: 0, end: 2 }; // original first-sentence span

    const sentence = screen.getByTestId("sentence-sent-0");
    fireEvent.doubleClick(sentence);

    const ta = screen.getByTestId("edit-sent-0") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "the deponent showed up here." } });
    fireEvent.blur(ta);

    const edited = latest.sentences.find((s) => s.id === "sent-0")!;
    const text = edited.words.map((w) => w.text).join(" ");
    expect(text).toBe("the deponent showed up here.");
    // re-spread keeps the original [start, end] span
    expect(edited.words[0]!.start).toBeCloseTo(span.start, 3);
    expect(edited.words.at(-1)!.end).toBeCloseTo(span.end, 3);
    for (const w of edited.words) {
      expect(w.start).toBeGreaterThanOrEqual(span.start - 1e-6);
      expect(w.end).toBeLessThanOrEqual(span.end + 1e-6);
    }
    // and the rendered DOM reflects the change
    expect(screen.getByTestId("sentence-sent-0")).toHaveTextContent("deponent");
  });

  it("click a speaker chip -> rename -> ALL that speaker's chips show new name", () => {
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
    const promptSpy = vi.spyOn(window, "prompt").mockReturnValue("Counsel");
    render(<H />);

    const chips = screen
      .getAllByTestId("speaker-chip")
      .filter((c) => c.getAttribute("data-speaker") === "SPEAKER_00");
    expect(chips.length).toBe(2); // two SPEAKER_00 turns
    fireEvent.click(chips[0]!);
    expect(promptSpy).toHaveBeenCalled();

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

  it("exports reflect edits + renames AND carry the disclaimer header", () => {
    function H() {
      const [state, setState] = useState<EditorState>(() => loadTranscript(FIXTURE));
      capture(state);
      return <Editor state={state} diarized setState={(s) => { capture(s); setState(s); }} />;
    }
    vi.spyOn(window, "prompt").mockReturnValue("Witness");
    render(<H />);

    // rename SPEAKER_00 -> Witness
    const chip = screen
      .getAllByTestId("speaker-chip")
      .find((c) => c.getAttribute("data-speaker") === "SPEAKER_00")!;
    fireEvent.click(chip);

    // edit sent-1
    fireEvent.doubleClick(screen.getByTestId("sentence-sent-1"));
    const ta = screen.getByTestId("edit-sent-1") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "I strongly disagree." } });
    fireEvent.blur(ta);

    const text = exportTextWithDisclaimer(latest);
    const json = exportJSONWithDisclaimer(latest);

    // disclaimer header present
    expect(text.startsWith("#")).toBe(true);
    expect(text).toContain(DRAFT_DISCLAIMER.split(". ")[0]!);
    expect(json.disclaimer).toBe(DRAFT_DISCLAIMER);

    // edits + renames reflected
    expect(text).toContain("Witness");
    expect(text).toContain("strongly disagree");
    expect(json.words.some((w) => w.speaker === "Witness")).toBe(true);
    expect(json.words.some((w) => w.word === "disagree.")).toBe(true);
    expect(json.words.some((w) => w.speaker === "SPEAKER_00")).toBe(false);
  });

  it("renders the persistent draft disclaimer banner", () => {
    render(<Harness onState={capture} />);
    expect(
      screen.getByLabelText("machine-generated draft disclaimer"),
    ).toHaveTextContent(DRAFT_DISCLAIMER.split(". ")[0]!);
  });

  it("click a word seeks the audio element", () => {
    render(<Harness onState={() => {}} />);
    const words = within(screen.getByTestId("sentence-sent-0")).getAllByTestId(
      "word",
    );
    // No audio src in this harness, so seek is a no-op but must not throw.
    expect(() => fireEvent.click(words[1]!)).not.toThrow();
  });

  it("paragraph mode (diarized=false): renders paragraphs, NO speaker chips, editing still commits", () => {
    render(<Harness onState={capture} diarized={false} />);

    // Paragraph blocks render, and there are NO speaker chips / reassign selects.
    expect(screen.getAllByTestId("paragraph").length).toBeGreaterThan(0);
    expect(screen.queryByTestId("speaker-chip")).toBeNull();
    expect(screen.queryByTestId("reassign-sent-0")).toBeNull();

    // Sentences still render and double-click-edit still commits.
    const sentence = screen.getByTestId("sentence-sent-0");
    fireEvent.doubleClick(sentence);
    const ta = screen.getByTestId("edit-sent-0") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "edited paragraph text." } });
    fireEvent.blur(ta);

    const edited = latest.sentences.find((s) => s.id === "sent-0")!;
    expect(edited.words.map((w) => w.text).join(" ")).toBe(
      "edited paragraph text.",
    );
    expect(screen.getByTestId("sentence-sent-0")).toHaveTextContent("edited");
  });

  it("diarized=true still shows speaker chips", () => {
    render(<Harness onState={capture} diarized={true} />);
    expect(screen.getAllByTestId("speaker-chip").length).toBeGreaterThan(0);
    expect(screen.queryByTestId("paragraph")).toBeNull();
  });
});
