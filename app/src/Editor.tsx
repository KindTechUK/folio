/**
 * Editor — the review-editor VIEW. It renders an EditorState (from core) and
 * dispatches every edit back through the core reducer; it never reimplements
 * edit/rename/reassign logic itself. The component is a thin render + event +
 * audio-playback layer over the tested `core/src/editor.ts`.
 *
 * NOTE: audio playback and the requestAnimationFrame highlight loop cannot run
 * in jsdom — they need a real webview. Verify those manually via
 * `npm run tauri dev`. The highlight INDEX selection is unit-tested via the pure
 * `activeWordIndex` in highlight.ts; everything else (edit/rename/reassign/export
 * wiring) is covered by the jsdom component test.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import {
  editSentence,
  editParagraph,
  joinSentences,
  splitSentence,
  renameSpeaker,
  reassignSentenceSpeaker,
  groupParagraphs,
} from "@core/editor";
import type { EditorState, EditorSentence } from "@core/editor";
import { UNKNOWN_SPEAKER } from "@core/types";
import { activeWordIndex, flatWords } from "./highlight";
import {
  DRAFT_DISCLAIMER,
  exportJSONWithDisclaimer,
  exportTextWithDisclaimer,
} from "./draft";
import { saveDocx } from "./docx";

const SPEEDS = [0.75, 1, 1.25, 1.5, 2] as const;

function mmss(t: number): string {
  const s = Math.max(0, t);
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

/** Deterministic per-speaker colour, ported from the prototype. */
function speakerColor(id: string): string {
  let h = 0;
  for (const c of id || "") h = (h * 31 + c.charCodeAt(0)) % 360;
  return `hsl(${h} 45% 38%)`;
}

/** A turn = a run of consecutive sentences sharing one speaker (positional id). */
interface TurnGroup {
  speaker: string;
  sentences: EditorSentence[];
}

function groupTurns(state: EditorState): TurnGroup[] {
  const turns: TurnGroup[] = [];
  for (const s of state.sentences) {
    const spk = s.words[0]?.speaker ?? UNKNOWN_SPEAKER;
    const prev = turns[turns.length - 1];
    if (prev && prev.speaker === spk) {
      prev.sentences.push(s);
    } else {
      turns.push({ speaker: spk, sentences: [s] });
    }
  }
  return turns;
}

function displayName(state: EditorState, id: string): string {
  return state.renameMap[id] ?? id;
}

function download(content: string, name: string, type: string): void {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([content], { type }));
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

export interface EditorProps {
  state: EditorState;
  setState: (next: EditorState) => void;
  /** Audio source URL (use `convertFileSrc(path)` in the Tauri app). */
  audioSrc?: string;
  /**
   * Whether the transcript was produced with speaker diarization. When `false`
   * (the default — paragraph QA mode) the document renders as plain pause-based
   * paragraphs with no speaker chips/reassign; when `true` it renders the
   * speaker-turn view. Karaoke highlight + double-click edit work in both.
   */
  diarized?: boolean;
}

export default function Editor({
  state,
  setState,
  audioSrc,
  diarized = false,
}: EditorProps) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(-1);
  const [cur, setCur] = useState(0);
  const [dur, setDur] = useState(0);
  const [speed, setSpeed] = useState(1);
  const [playing, setPlaying] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const turns = useMemo(() => groupTurns(state), [state]);
  const paragraphs = useMemo(
    () => groupParagraphs(state.sentences),
    [state],
  );
  const starts = useMemo(() => flatWords(state).map((w) => w.start), [state]);

  // rAF highlight loop. Cannot run headlessly (no real <audio>), so this is the
  // manually-verified path; the index logic itself is the unit-tested
  // activeWordIndex.
  useEffect(() => {
    let raf = 0;
    const loop = () => {
      const a = audioRef.current;
      if (a) {
        const t = a.currentTime;
        setCur(t);
        if (editingId === null) {
          const idx = activeWordIndex(starts, t);
          setActiveIdx((prev) => (prev === idx ? prev : idx));
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [starts, editingId]);

  function seek(t: number): void {
    const a = audioRef.current;
    if (a) a.currentTime = t;
  }

  function togglePlay(): void {
    const a = audioRef.current;
    if (!a) return;
    if (a.paused) void a.play();
    else a.pause();
  }

  function applySpeed(s: number): void {
    setSpeed(s);
    const a = audioRef.current;
    if (a) a.playbackRate = s;
  }

  function commitEdit(id: string, text: string): void {
    setState(editSentence(state, id, text));
    setEditingId(null);
  }

  // Paragraph-mode commit: re-spread the whole paragraph's text across its
  // anchor span and collapse the run to one sentence (core `editParagraph`).
  function commitParagraphEdit(sentenceIds: string[], text: string): void {
    setState(editParagraph(state, sentenceIds, text));
    setEditingId(null);
  }

  // Merge this paragraph's sentences with the next paragraph's into one run
  // (core `joinSentences`); timings unchanged.
  function joinWithNext(para: EditorSentence[], nextPara: EditorSentence[]): void {
    setState(joinSentences(state, [...para, ...nextPara].map((s) => s.id)));
  }

  // Split a sentence at a word boundary (core `splitSentence`); timings intact.
  function splitAt(sentenceId: string, wordIndex: number): void {
    setState(splitSentence(state, sentenceId, wordIndex));
  }

  function onRename(origId: string): void {
    const current = displayName(state, origId);
    const name = window.prompt(`Rename "${current}" everywhere as:`, current);
    if (name == null) return;
    setState(renameSpeaker(state, origId, name.trim() || origId));
  }

  function onReassign(sentenceId: string, newId: string): void {
    if (!newId.trim()) return;
    setState(reassignSentenceSpeaker(state, sentenceId, newId.trim()));
  }

  // Flat index bookkeeping so each rendered word knows its document position
  // (for highlight). Built in the same document order as flatWords/starts —
  // both views walk state.sentences in order, so the cursor stays consistent.
  let flatCursor = 0;

  // Render the words of a sentence as click-to-seek, highlight-aware spans.
  // `flatCursor` advances in document order so the flat highlight index stays
  // aligned with flatWords/starts (the karaoke engine is untouched). Shared by
  // the per-sentence (speaker) view and the per-paragraph (plain) read view.
  function renderWords(
    sent: EditorSentence,
    onSplitBefore?: (wordIndex: number) => void,
  ): JSX.Element[] {
    return sent.words.map((w, wi) => {
      const myIdx = flatCursor++;
      const active = myIdx === activeIdx;
      const played = activeIdx >= 0 && myIdx < activeIdx;
      return (
        <span
          key={wi}
          className={`w${active ? " active" : ""}`}
          data-testid="word"
          onClick={(ev) => {
            ev.stopPropagation();
            // Shift-click splits the sentence just before this word (paragraph
            // mode only); plain click seeks. Splitting at index 0 is a no-op in
            // core, so leading-word shift-clicks safely do nothing.
            if (onSplitBefore && ev.shiftKey) {
              onSplitBefore(wi);
              return;
            }
            seek(w.start);
          }}
          style={{
            cursor: "pointer",
            borderRadius: 4,
            background: active ? "#fde047" : undefined,
            color: active ? "#422006" : played ? "#999" : undefined,
          }}
        >
          {w.text}{" "}
        </span>
      );
    });
  }

  // Render ONE sentence (editable on double-click; per-word highlight). Used by
  // the speaker-turn view so per-sentence editing + reassign stay as-is.
  function renderSentence(sent: EditorSentence): JSX.Element {
    const isEditing = editingId === sent.id;
    if (isEditing) {
      return (
        <SentenceEditor
          key={sent.id}
          sent={sent}
          onCommit={(text) => commitEdit(sent.id, text)}
          onCancel={() => setEditingId(null)}
        />
      );
    }
    return (
      <span
        key={sent.id}
        className="sent"
        data-testid={`sentence-${sent.id}`}
        onDoubleClick={() => setEditingId(sent.id)}
        style={{ borderRadius: 6, padding: "1px 3px" }}
      >
        {renderWords(sent)}
      </span>
    );
  }

  return (
    <div className="editor">
      <div
        role="note"
        aria-label="machine-generated draft disclaimer"
        style={{
          background: "#fff8e1",
          border: "1px solid #c08a00",
          color: "#7a5800",
          padding: "0.5rem 0.8rem",
          borderRadius: 6,
          fontSize: 13,
          marginBottom: 12,
        }}
      >
        {DRAFT_DISCLAIMER}
      </div>

      <div
        className="transport"
        style={{
          display: "flex",
          gap: 10,
          alignItems: "center",
          flexWrap: "wrap",
          position: "sticky",
          top: 0,
          background: "#fff",
          padding: "8px 0",
          borderBottom: "1px solid #ddd",
          zIndex: 5,
        }}
      >
        {audioSrc && (
          // eslint-disable-next-line jsx-a11y/media-has-caption
          <audio
            ref={audioRef}
            src={audioSrc}
            preload="metadata"
            data-testid="audio"
            onLoadedMetadata={(e) => setDur(e.currentTarget.duration)}
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
          />
        )}
        <button onClick={togglePlay} data-testid="play">
          {playing ? "Pause" : "Play"}
        </button>
        <span style={{ fontVariantNumeric: "tabular-nums", color: "#555" }}>
          {mmss(cur)} / {mmss(dur)}
        </span>
        <span style={{ display: "flex", gap: 4, alignItems: "center" }}>
          <span style={{ fontSize: 12, color: "#888" }}>speed</span>
          {SPEEDS.map((s) => (
            <button
              key={s}
              onClick={() => applySpeed(s)}
              aria-pressed={speed === s}
              style={{ fontWeight: speed === s ? 700 : 400 }}
            >
              {s}x
            </button>
          ))}
        </span>
        <button
          data-testid="export-json"
          onClick={() =>
            download(
              JSON.stringify(exportJSONWithDisclaimer(state), null, 2),
              "transcript_edited.json",
              "application/json",
            )
          }
        >
          Export JSON
        </button>
        <button
          data-testid="export-text"
          onClick={() =>
            download(
              exportTextWithDisclaimer(state),
              "transcript_edited.txt",
              "text/plain",
            )
          }
        >
          Export Script
        </button>
        <button
          data-testid="export-docx"
          onClick={() => {
            void saveDocx(state, { diarized });
          }}
        >
          Export DOCX
        </button>
      </div>

      <div className="doc" style={{ maxWidth: 820, margin: "16px auto" }}>
        {!diarized
          ? paragraphs.map((para, pi) => {
              // A paragraph collapses to one sentence on edit (id = first
              // sentence's id), so we key paragraph edit state on that id. This
              // shares the same `editingId` gate as the speaker view, so the
              // rAF highlight freezes/resumes identically.
              const paraId = para[0]!.id;
              const isEditing = editingId === paraId;
              if (isEditing) {
                return (
                  <ParagraphEditor
                    key={paraId}
                    para={para}
                    onCommit={(text) =>
                      commitParagraphEdit(
                        para.map((s) => s.id),
                        text,
                      )
                    }
                    onCancel={() => setEditingId(null)}
                  />
                );
              }
              const next = paragraphs[pi + 1];
              return (
                <p
                  key={paraId}
                  className="paragraph"
                  data-testid="paragraph"
                  data-paragraph-id={paraId}
                  onDoubleClick={() => setEditingId(paraId)}
                  style={{ margin: "0 0 16px", lineHeight: 1.6 }}
                >
                  {para.map((sent) => (
                    <span
                      key={sent.id}
                      className="sent"
                      data-testid={`sentence-${sent.id}`}
                    >
                      {renderWords(sent, (wi) => splitAt(sent.id, wi))}
                    </span>
                  ))}
                  {next && (
                    <button
                      data-testid={`join-${paraId}`}
                      aria-label="join with next paragraph"
                      title="Join with next paragraph"
                      onClick={(ev) => {
                        ev.stopPropagation();
                        joinWithNext(para, next);
                      }}
                      style={{
                        marginLeft: 6,
                        fontSize: 11,
                        color: "#888",
                        background: "none",
                        border: "1px solid #ddd",
                        borderRadius: 4,
                        cursor: "pointer",
                      }}
                    >
                      ⌄ join
                    </button>
                  )}
                </p>
              );
            })
          : turns.map((turn, ti) => {
          const color = speakerColor(turn.speaker);
          const first = turn.sentences[0]?.words[0];
          return (
            <div
              key={ti}
              className="turn"
              style={{
                margin: "0 0 18px",
                paddingLeft: 12,
                borderLeft: `3px solid ${color}`,
              }}
            >
              <div
                style={{
                  display: "flex",
                  gap: 10,
                  alignItems: "baseline",
                  marginBottom: 2,
                }}
              >
                <span
                  role="button"
                  tabIndex={0}
                  className="spk"
                  data-testid="speaker-chip"
                  data-speaker={turn.speaker}
                  onClick={() => onRename(turn.speaker)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") onRename(turn.speaker);
                  }}
                  style={{
                    fontWeight: 700,
                    cursor: "pointer",
                    padding: "1px 8px",
                    borderRadius: 6,
                    background: color,
                    color: "#fff",
                    fontSize: 13,
                  }}
                >
                  {displayName(state, turn.speaker)}
                </span>
                {first && (
                  <span style={{ color: "#888", fontSize: 12 }}>
                    {mmss(first.start)}
                  </span>
                )}
                <select
                  aria-label="reassign speaker"
                  data-testid={`reassign-${turn.sentences[0]?.id ?? ti}`}
                  value=""
                  onChange={(e) => {
                    const v = e.target.value;
                    if (v) onReassign(turn.sentences[0]!.id, v);
                    e.target.value = "";
                  }}
                  style={{ fontSize: 11 }}
                >
                  <option value="">reassign…</option>
                  {Array.from(
                    new Set([
                      ...Object.keys(state.renameMap),
                      ...state.sentences.map(
                        (s) => s.words[0]?.speaker ?? UNKNOWN_SPEAKER,
                      ),
                    ]),
                  ).map((id) => (
                    <option key={id} value={id}>
                      {displayName(state, id)}
                    </option>
                  ))}
                </select>
              </div>

              <div className="body">
                {turn.sentences.map((sent) => renderSentence(sent))}
              </div>
            </div>
          );
            })}
      </div>
    </div>
  );
}

/**
 * Inline editable text for a WHOLE paragraph (paragraph mode). Seeded with the
 * paragraph's joined text; Enter/blur commits via `editParagraph`, Esc cancels.
 * Keyed (testid) on the paragraph's first-sentence id, which is also the id the
 * collapsed sentence keeps, so the highlight/edit id stays stable.
 */
function ParagraphEditor({
  para,
  onCommit,
  onCancel,
}: {
  para: EditorSentence[];
  onCommit: (text: string) => void;
  onCancel: () => void;
}) {
  const paraId = para[0]!.id;
  const initial = para.flatMap((s) => s.words.map((w) => w.text)).join(" ");
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement | null>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <p className="paragraph editing" style={{ margin: "0 0 16px" }}>
      <textarea
        ref={ref}
        className="sent editing"
        data-testid={`edit-${paraId}`}
        aria-label="edit paragraph"
        value={text}
        rows={4}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => onCommit(text)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            onCommit(text);
          } else if (e.key === "Escape") {
            e.preventDefault();
            onCancel();
          }
        }}
        style={{
          display: "block",
          width: "100%",
          font: "inherit",
          lineHeight: 1.6,
          padding: "6px 8px",
          boxSizing: "border-box",
          boxShadow: "0 0 0 2px #38bdf8",
          borderRadius: 6,
        }}
      />
    </p>
  );
}

/** Inline editable text for one sentence. Enter/blur commits, Esc cancels. */
function SentenceEditor({
  sent,
  onCommit,
  onCancel,
}: {
  sent: EditorSentence;
  onCommit: (text: string) => void;
  onCancel: () => void;
}) {
  const initial = sent.words.map((w) => w.text).join(" ");
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement | null>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <textarea
      ref={ref}
      className="sent editing"
      data-testid={`edit-${sent.id}`}
      value={text}
      rows={2}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => onCommit(text)}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          onCommit(text);
        } else if (e.key === "Escape") {
          e.preventDefault();
          onCancel();
        }
      }}
      style={{
        display: "block",
        width: "100%",
        font: "inherit",
        padding: "6px 8px",
        boxSizing: "border-box",
        boxShadow: "0 0 0 2px #38bdf8",
        borderRadius: 6,
      }}
    />
  );
}
