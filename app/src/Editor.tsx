/**
 * Review-editor view: renders an EditorState and dispatches every edit through
 * the core reducer (core/src/editor.ts).
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
  replaceAll,
  countReplacements,
} from "@core/editor";
import type { EditorState, EditorSentence } from "@core/editor";
import { UNKNOWN_SPEAKER } from "@core/types";
import { activeWordIndex, flatWords } from "./highlight";
import {
  draftDisclaimer,
  exportJSONWithDisclaimer,
  exportTextWithDisclaimer,
} from "./draft";
import { saveDocx } from "./docx";
import { displayName, mmss } from "./format";
import { Banner, Button } from "./ui";
import { Toolbar, ToolbarGroup } from "./ui/Toolbar";

const SPEEDS = [0.75, 1, 1.25, 1.5, 2] as const;

/** Deterministic per-speaker colour. */
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
  /** App-owned edit history. Optional so the Editor can render without it. */
  onUndo?: () => void;
  onRedo?: () => void;
  canUndo?: boolean;
  canRedo?: boolean;
}

export default function Editor({
  state,
  setState,
  audioSrc,
  diarized = false,
  onUndo,
  onRedo,
  canUndo = false,
  canRedo = false,
}: EditorProps) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(-1);
  const [cur, setCur] = useState(0);
  const [dur, setDur] = useState(0);
  const [speed, setSpeed] = useState(1);
  const [playing, setPlaying] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  // Positional id of the speaker chip being renamed, or null.
  const [renamingSpeaker, setRenamingSpeaker] = useState<string | null>(null);
  const [findOpen, setFindOpen] = useState(false);
  const [findText, setFindText] = useState("");
  const [replaceText, setReplaceText] = useState("");
  const [findCase, setFindCase] = useState(false);
  const [findWhole, setFindWhole] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [follow, setFollow] = useState(true);

  const turns = useMemo(() => groupTurns(state), [state]);
  const paragraphs = useMemo(
    () => groupParagraphs(state.sentences),
    [state],
  );
  const starts = useMemo(() => flatWords(state).map((w) => w.start), [state]);

  // Live match count without building the replaced state; the real replace runs on
  // "Replace all".
  const findOpts = useMemo(
    () => ({ caseSensitive: findCase, wholeWord: findWhole }),
    [findCase, findWhole],
  );
  const findCount = useMemo(
    () => countReplacements(state, findText, replaceText, findOpts),
    [state, findText, replaceText, findOpts],
  );

  // rAF playback-highlight loop; the highlight is frozen while a sentence is being edited.
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

  // Auto-follow: block:"nearest" only — smooth scroll lags at high playback speed.
  useEffect(() => {
    if (playing && follow && activeIdx >= 0) {
      document
        .querySelector(`[data-flat-index="${activeIdx}"]`)
        ?.scrollIntoView({ block: "nearest" });
    }
  }, [activeIdx, playing, follow]);

  // Global shortcuts: Cmd/Ctrl-Z undo, Cmd/Ctrl-Shift-Z or Ctrl-Y redo, Space
  // play/pause. Ignored while editing or when a text field is focused (native
  // editing wins); Space is also ignored on a focused BUTTON so keyboard
  // activation still works.
  useEffect(() => {
    function isTextTarget(el: Element | null, includeButton: boolean): boolean {
      if (!el) return false;
      const tag = el.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
      if (includeButton && tag === "BUTTON") return true;
      if ((el as HTMLElement).isContentEditable) return true;
      return false;
    }
    function onKeyDown(e: KeyboardEvent): void {
      if (editingId !== null) return;
      const active = document.activeElement;
      const meta = e.metaKey || e.ctrlKey;
      const key = e.key.toLowerCase();

      if (meta && key === "z" && !e.shiftKey) {
        if (isTextTarget(active, false)) return; // native text undo wins
        if (canUndo) {
          e.preventDefault();
          onUndo?.();
        }
        return;
      }
      if ((meta && key === "z" && e.shiftKey) || (e.ctrlKey && key === "y")) {
        if (isTextTarget(active, false)) return; // native text redo wins
        if (canRedo) {
          e.preventDefault();
          onRedo?.();
        }
        return;
      }
      if (e.key === " ") {
        if (isTextTarget(active, true)) return;
        e.preventDefault();
        togglePlay();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [editingId, canUndo, canRedo, onUndo, onRedo]);

  useEffect(() => {
    if (!exportOpen) return;
    function onPointerDown(e: PointerEvent): void {
      if (!(e.target as Element | null)?.closest(".export-menu")) setExportOpen(false);
    }
    function onKeyDown(e: KeyboardEvent): void {
      if (e.key === "Escape") setExportOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [exportOpen]);

  // Clamped to [0, duration]; duration is NaN until metadata loads.
  function seek(t: number): void {
    const a = audioRef.current;
    if (!a) return;
    const end = Number.isFinite(a.duration) ? a.duration : Infinity;
    a.currentTime = Math.max(0, Math.min(t, end));
  }

  function skip(seconds: number): void {
    const a = audioRef.current;
    if (a) seek(a.currentTime + seconds);
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

  function commitParagraphEdit(sentenceIds: string[], text: string): void {
    setState(editParagraph(state, sentenceIds, text));
    setEditingId(null);
  }

  function joinWithNext(para: EditorSentence[], nextPara: EditorSentence[]): void {
    setState(joinSentences(state, [...para, ...nextPara].map((s) => s.id)));
  }

  function splitAt(sentenceId: string, wordIndex: number): void {
    setState(splitSentence(state, sentenceId, wordIndex));
  }

  // Empty input renames back to the positional id.
  function commitRename(origId: string, value: string): void {
    setState(renameSpeaker(state, origId, value.trim() || origId));
    setRenamingSpeaker(null);
  }

  function onReassign(sentenceId: string, newId: string): void {
    if (!newId.trim()) return;
    setState(reassignSentenceSpeaker(state, sentenceId, newId.trim()));
  }

  // Advances in document order so each word's index matches `starts` (flatWords order).
  let flatCursor = 0;

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
          data-flat-index={myIdx}
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
            background: active ? "var(--highlight-active)" : undefined,
            color: active
              ? "var(--highlight-active-text)"
              : played
                ? "var(--slate-400)"
                : undefined,
          }}
        >
          {w.text}{" "}
        </span>
      );
    });
  }

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
      {audioSrc && (
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

      <Toolbar sticky style={{ justifyContent: "flex-end", padding: "8px 0" }}>
        <ToolbarGroup>
          <Button size="sm" data-testid="undo" onClick={() => onUndo?.()} disabled={!canUndo}>
            Undo
          </Button>
          <Button size="sm" data-testid="redo" onClick={() => onRedo?.()} disabled={!canRedo}>
            Redo
          </Button>
        </ToolbarGroup>
        <ToolbarGroup>
          <Button
            size="sm"
            variant={findOpen ? "primary" : "ghost"}
            data-testid="toggle-find"
            onClick={() => setFindOpen((v) => !v)}
            aria-pressed={findOpen}
          >
            Find &amp; replace
          </Button>
        </ToolbarGroup>
        <ToolbarGroup divider={false}>
          <div className="export-menu">
            <Button
              size="sm"
              variant="primary"
              data-testid="export-menu"
              aria-haspopup="menu"
              aria-expanded={exportOpen}
              onClick={() => setExportOpen((open) => !open)}
            >
              Export
            </Button>
            {exportOpen && (
              <div className="export-menu-popover" role="menu">
                <button
                  role="menuitem"
                  data-testid="export-docx"
                  onClick={() => {
                    setExportOpen(false);
                    void saveDocx(state, { diarized });
                  }}
                >
                  Word document (.docx)
                </button>
                <button
                  role="menuitem"
                  data-testid="export-text"
                  onClick={() => {
                    setExportOpen(false);
                    download(
                      exportTextWithDisclaimer(state, diarized),
                      "transcript_edited.txt",
                      "text/plain",
                    );
                  }}
                >
                  Plain text (.txt)
                </button>
                <button
                  role="menuitem"
                  data-testid="export-json"
                  onClick={() => {
                    setExportOpen(false);
                    download(
                      JSON.stringify(exportJSONWithDisclaimer(state, diarized), null, 2),
                      "transcript_edited.json",
                      "application/json",
                    );
                  }}
                >
                  Transcript data (.json)
                </button>
              </div>
            )}
          </div>
        </ToolbarGroup>
      </Toolbar>

      {findOpen && (
        <div
          className="find-replace"
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            flexWrap: "wrap",
            padding: "8px 0",
            borderBottom: "1px solid var(--slate-200)",
          }}
        >
          <input
            data-testid="find-input"
            placeholder="Find"
            value={findText}
            onChange={(e) => setFindText(e.target.value)}
            style={{ font: "inherit", padding: "4px 6px" }}
          />
          <input
            data-testid="replace-input"
            placeholder="Replace with"
            value={replaceText}
            onChange={(e) => setReplaceText(e.target.value)}
            style={{ font: "inherit", padding: "4px 6px" }}
          />
          <label style={{ display: "flex", gap: 4, alignItems: "center", fontSize: 12 }}>
            <input
              type="checkbox"
              data-testid="find-case"
              checked={findCase}
              onChange={(e) => setFindCase(e.target.checked)}
            />
            Match case
          </label>
          <label style={{ display: "flex", gap: 4, alignItems: "center", fontSize: 12 }}>
            <input
              type="checkbox"
              data-testid="find-whole"
              checked={findWhole}
              onChange={(e) => setFindWhole(e.target.checked)}
            />
            Whole word
          </label>
          <span data-testid="find-count" style={{ fontSize: "var(--text-xs)", color: "var(--slate-500)" }}>
            {findCount} matches
          </span>
          <Button
            size="sm"
            variant="primary"
            data-testid="replace-all"
            disabled={findText === "" || findCount === 0}
            onClick={() =>
              setState(replaceAll(state, findText, replaceText, findOpts).state)
            }
          >
            Replace all
          </Button>
        </div>
      )}

      <div className="draft-guidance">
        <Banner variant="info" role="note" aria-label="machine-generated draft disclaimer">
          {draftDisclaimer(diarized)}
        </Banner>
        <div className="correction-hint">
          <strong>Quick correction</strong>
          <span>Click a word to replay · Double-click a paragraph to edit · Space to play or pause</span>
        </div>
      </div>

      {/* Empty-space clicks toggle play; word, chip, button and editor handlers
          stopPropagation. */}
      <div
        className="doc"
        onClick={togglePlay}
        style={{ margin: "16px auto 28px" }}
      >
        {!diarized
          ? paragraphs.map((para, pi) => {
              // Edit state is keyed on the first sentence's id — the id the
              // collapsed sentence keeps.
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
                        color: "var(--slate-500)",
                        background: "none",
                        border: "1px solid var(--slate-200)",
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
          // A speaker can appear in multiple turns; rename is global, so when
          // `renamingSpeaker` matches we only mount the inline input on the
          // FIRST turn for that speaker. Mounting it on every matching turn
          // would race two autofocus inputs (the later mount's focus blurs the
          // earlier, committing prematurely).
          const isRenameTurn =
            renamingSpeaker === turn.speaker &&
            turns.findIndex((t) => t.speaker === turn.speaker) === ti;
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
                {isRenameTurn ? (
                  <SpeakerRenameInput
                    initial={displayName(state, turn.speaker)}
                    onCommit={(value) => commitRename(turn.speaker, value)}
                    onCancel={() => setRenamingSpeaker(null)}
                  />
                ) : (
                  <span
                    role="button"
                    tabIndex={0}
                    className="spk"
                    data-testid="speaker-chip"
                    data-speaker={turn.speaker}
                    onClick={(e) => {
                      e.stopPropagation();
                      setRenamingSpeaker(turn.speaker);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        e.stopPropagation();
                        setRenamingSpeaker(turn.speaker);
                      }
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
                )}
                {first && (
                  <span style={{ color: "var(--slate-500)", fontSize: 12 }}>
                    {mmss(first.start)}
                  </span>
                )}
                <select
                  aria-label="reassign speaker"
                  data-testid={`reassign-${turn.sentences[0]?.id ?? ti}`}
                  value=""
                  onClick={(e) => e.stopPropagation()}
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

      <div className="transport-bar" role="group" aria-label="Transcript playback">
        <Button variant="primary" size="sm" onClick={togglePlay} data-testid="play">
          {playing ? "Pause" : "Play"}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => skip(-5)} aria-label="Back 5 seconds">
          −5s
        </Button>
        <Button size="sm" variant="ghost" onClick={() => skip(5)} aria-label="Forward 5 seconds">
          +5s
        </Button>
        <span className="tabular transport-time">
          {mmss(cur)} / {mmss(dur)}
        </span>
        <input
          className="transport-progress"
          type="range"
          min={0}
          max={dur || 0}
          step={0.1}
          value={Math.min(cur, dur || 0)}
          onChange={(event) => seek(Number(event.target.value))}
          aria-label="Playback position"
          disabled={!audioSrc || dur === 0}
        />
        <select
          aria-label="Playback speed"
          value={speed}
          onChange={(event) => applySpeed(Number(event.target.value))}
        >
          {SPEEDS.map((option) => (
            <option key={option} value={option}>
              {option}x
            </option>
          ))}
        </select>
        <Button
          size="sm"
          variant={follow ? "primary" : "ghost"}
          data-testid="toggle-follow"
          onClick={() => setFollow((value) => !value)}
          aria-pressed={follow}
        >
          Follow
        </Button>
      </div>
    </div>
  );
}

/** Inline paragraph editor. Enter/blur commits via `editParagraph`, Esc cancels. */
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
        onClick={(e) => e.stopPropagation()}
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
          boxShadow: "var(--ring)",
          borderRadius: 6,
        }}
      />
    </p>
  );
}

/**
 * Inline speaker-rename input shown in place of the chip. Enter/blur commits, Esc
 * cancels. Click stopPropagation keeps the background play-toggle out of the way.
 */
function SpeakerRenameInput({
  initial,
  onCommit,
  onCancel,
}: {
  initial: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement | null>(null);
  // Enter commits then unmounts the input, which fires blur — guard so we only
  // commit/cancel once (no duplicate history entry).
  const done = useRef(false);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const commit = (): void => {
    if (done.current) return;
    done.current = true;
    onCommit(value);
  };
  const cancel = (): void => {
    if (done.current) return;
    done.current = true;
    onCancel();
  };
  return (
    <input
      ref={ref}
      data-testid="speaker-rename-input"
      aria-label="rename speaker"
      value={value}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit();
        } else if (e.key === "Escape") {
          e.preventDefault();
          cancel();
        }
      }}
      style={{
        font: "inherit",
        fontWeight: 700,
        fontSize: 13,
        padding: "1px 6px",
        borderRadius: 6,
        boxShadow: "var(--ring)",
      }}
    />
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
      onClick={(e) => e.stopPropagation()}
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
        boxShadow: "var(--ring)",
        borderRadius: 6,
      }}
    />
  );
}
