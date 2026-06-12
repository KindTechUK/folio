# Paragraph-mode editing UX

Status: design proposal (research-grounded). No code changed by this doc.

## Problem

When diarization is OFF (the new default), the transcript renders as pause-based
paragraphs (`groupParagraphs`) with a karaoke word-highlight during playback. Editing
today is **sentence-level**: double-click one sentence -> it becomes a `<textarea>` ->
Enter/blur commits via `editSentence`, which re-spreads the sentence's original
`[start,end]` across the re-tokenized words by character length.

User feedback: in paragraph mode this feels wrong. Users want **fluid word/paragraph
text editing** - join sentences into a paragraph, edit a whole paragraph continuously
(not one sentence at a time) - while keeping the karaoke hint and click-a-word-to-seek.
Word<->time alignment is a **playback aid only** (+/-200-500ms); the user must never
maintain timestamps by hand.

## Grounding in our system

- `core/src/editor.ts`
  - State: `sentences: { id, words:[{text,start,end,speaker}] }[]` + `renameMap`.
  - `editSentence(state, id, text)`: re-tokenize `text`, re-spread the sentence's
    **original** `[start,end]` span across new tokens weighted by char length. Empty text
    deletes the sentence. This is the timing-after-edit philosophy we keep.
  - `groupParagraphs(sentences, gap=0.75, max=6)`: pure grouping of sentences into
    paragraphs by pause/cap. Layout only - it does not change the underlying sentences.
- `app/src/highlight.ts`
  - `flatWords(state)` = all words in document order; `activeWordIndex(starts, t)` =
    binary search for the last word with `start <= t`. The highlight is purely
    **index over a flat, document-ordered word list**.
- `app/src/Editor.tsx`
  - rAF loop sets `activeIdx`; each rendered word knows its flat index via a `flatCursor`
    that walks `state.sentences` in the **same order** as `flatWords`. Word `onClick` ->
    `seek(w.start)`. Editing freezes the highlight (`if (editingId === null)`).
  - Key invariant: **highlight correctness depends only on rendering words in document
    order with monotonically non-decreasing starts.** Any design that preserves that
    invariant keeps the karaoke + click-to-seek working for free.

## Prior-art survey (current behaviour, 2026)

| App | Editing granularity | Surface | Timing after a free-form edit | Highlight approach |
|---|---|---|---|---|
| **Descript** | Whole transcript, edit-like-a-doc | One contiguous rich-text doc (word-backed) | Edits move the media with the text; **re-runs forced alignment** on changed spans (dotted underline while re-aligning); manual wordbar nudge as fallback | Word-synced playhead over the doc; each word carries timing |
| **Reduct.video** | Doc-style; select-a-span correction window; double-click a word | Contiguous transcript; advanced mode shows waveform | "Edit like a doc, Reduct takes care of timestamps"; advanced mode lets you drag word boundaries; insert words on the soundwave | Word-level timestamps drive transcript navigation/highlight |
| **oTranscribe** | Plain free text; no per-word model | Single `<textarea>` next to player | No word timings at all; user inserts clickable **timestamp markers** via shortcut | No karaoke; timestamp links only |
| **Subtitle Edit** | Cue/line + waveform | Discrete cue list + waveform | Manual: drag cue edges on waveform; point-sync / global shift / AI snap-to-waveform | Per-cue, waveform-anchored (not word karaoke) |
| **Aiko** | None (export to edit) | Read-only | n/a | Read-along only |
| **MacWhisper** | Segment-level edit; read-along | Discrete segments | Keeps segment `[start,end]`; text edits don't re-derive word times (segment-granular) | Segment/word read-along |
| **Hyprnote / Char** | Rich note editor (ProseMirror), word-level timestamps on-device | Contiguous ProseMirror doc | Note text is decoupled from the raw transcript; word timestamps attached as metadata | Word-level highlight over the editor |
| **otter.ai** | Enter edit mode, type freely; tap word to seek | Contiguous paragraphs | "Sync text to audio" step **re-aligns** after edits | First word per paragraph highlighted; follows playback; tap word to seek |

### Key insights from the survey

1. **The market split is binary.** Either (a) plain text with no per-word timing
   (oTranscribe), or (b) edit-like-a-doc with **word-backed contiguous surfaces** that
   **re-derive timing after edits**. Nobody who keeps a karaoke highlight makes the user
   maintain timestamps - they all re-align or interpolate. Sentence-at-a-time textareas
   (our current model) are an outlier and are exactly what feels wrong.
2. **The serious editors re-align with the audio** (Descript, otter "sync text to audio",
   Subtitle Edit AI-snap). We have no forced-aligner in the review loop and don't want one
   on the CPU-only path, so our honest equivalent is **interpolation/re-spread within a
   known anchored span** - which is precisely what `editSentence` already does. That makes
   our existing reducer philosophy the *correct* one; we only need to widen its unit of
   work from a sentence to a paragraph.
3. **Contiguous beats blocks for "feels like a doc."** Descript/Reduct/otter/Hyprnote are
   all one continuous editable surface. Discrete per-sentence blocks are what produce the
   "one sentence at a time" complaint.
4. **Highlight over an editable surface is universally done by anchoring timing to word
   spans, not characters**, and freezing/refreshing the index on edit. This matches our
   flat-index model exactly.

## Design options

All three keep the disclaimer banner, transport, exports, and the `diarized=true`
speaker-turn view untouched. They differ only in the `diarized=false` paragraph view.

### Option A - Per-paragraph contiguous edit (re-spread on commit) [recommended]

Make the **paragraph** the edit unit. Render each paragraph as word spans (highlight +
click-to-seek) exactly as today. Double-click anywhere in a paragraph turns the **whole
paragraph** into a single multi-line editable surface seeded with the paragraph's full
text. On commit, a new core op re-tokenizes the whole paragraph and re-spreads timing
across the paragraph's anchored `[start,end]` span - the `editSentence` philosophy lifted
to paragraph scope - then collapses the paragraph's sentences into one sentence (or
re-splits into sentences; see below).

- **Surface**: contiguous *within a paragraph*; paragraphs stay as separate `<p>` blocks
  (paragraph boundaries are meaningful = the pause structure / reading rhythm).
- **Timing after edit**: re-spread the paragraph's first-word `start` -> last-word `end`
  across new tokens by char length. No timestamp work for the user. Cross-paragraph
  boundaries (the pauses) are preserved because each paragraph keeps its own anchor span.
- **Highlight survival**: words still render in document order with non-decreasing starts,
  so `flatWords`/`activeWordIndex` are unchanged. Highlight freezes while a paragraph is
  in edit mode (same `editingId` gate, keyed by paragraph id) and resumes on commit.
- **Join/split**: "join with next paragraph" = stop treating the gap as a break for these
  two groups; implemented as merging their sentences into one paragraph unit. Splitting a
  paragraph at the cursor = split the underlying sentence at a word boundary. Both become
  core ops.
- **Pros**: smallest leap from today; reuses the proven re-spread; zero change to the
  highlight engine; "edit the whole paragraph continuously" directly answers the feedback;
  trivially testable headless.
- **Cons**: editing is still modal (double-click to enter, commit to exit) rather than
  always-on typing; join/split need explicit affordances; a paragraph that spans a real
  pause gets its internal pause structure flattened to a single re-spread span on edit
  (acceptable: +/-200-500ms aid).

### Option B - Per-paragraph `<textarea>` with join/split buttons

Same as A but the edit surface is a plain `<textarea>` (no word spans while editing) and
join/split are explicit buttons between paragraphs. Essentially today's `SentenceEditor`
promoted to paragraph scope plus chrome.

- **Pros**: dead simple; least risky; reuses existing `SentenceEditor` almost verbatim.
- **Cons**: loses click-to-seek and karaoke *while editing* (textarea has no spans);
  button chrome between paragraphs is clunky; feels least like "fluid doc editing".

### Option C - Descript-style single whole-document contenteditable

One `contenteditable` (or ProseMirror/Lexical) surface for the entire transcript; word
`<span>`s with `data-start`; always-on typing; on blur/idle, diff the DOM word list
against state and re-spread/interpolate timing per affected region.

- **Pros**: the truest "edit like a doc" feel; matches Descript/Hyprnote.
- **Cons**: highest risk to the **tested reducer** and the highlight invariant. Mapping
  arbitrary contenteditable mutations (paste, multi-word delete, cross-paragraph merges)
  back onto `sentences[]` while preserving document order and anchor spans is a large,
  fiddly, hard-to-unit-test job; cursor/IME/undo handling in a Tauri webview is a project
  on its own. Over-built for a review-and-correct tool where edits are mostly fix-a-word
  and merge-a-paragraph, not heavy rewriting.

## Recommendation

**Adopt Option A.** It makes the paragraph the unit of editing - so users edit a whole
paragraph continuously instead of one sentence at a time - while keeping the karaoke
highlight and click-to-seek completely unchanged (words still render in document order;
the flat-index engine is untouched). It reuses the already-tested re-spread philosophy of
`editSentence`, so the risk to the proven `core` reducer is minimal: we add new pure ops
with TDD rather than rewriting the highlight or adopting a heavyweight rich-text engine.
Option C is the long-term "Descript feel" but is disproportionate risk for a CPU-only
review tool; Option B loses the in-place karaoke that users explicitly want to keep.

Rationale against the survey: the credible doc-style editors all (a) use a word-backed
contiguous surface and (b) re-derive timing after edits rather than asking the user to
maintain it. Option A delivers both at paragraph granularity with interpolation standing
in for the forced-aligner we deliberately don't run.

## Implementation sketch (next editor PR)

### New `core/src/editor.ts` ops (TDD first, in `core/test/editor.test.ts`)

1. `editParagraph(state, sentenceIds: string[], newText: string): EditorState`
   - Anchor span = first word `start` of the first listed sentence -> last word `end` of
     the last. Re-tokenize `newText`, re-spread across the span by char length (reuse the
     exact `editSentence` spread math; factor it into a private `respread(tokens, s, e,
     speaker)` helper used by both). Speaker = first word's speaker.
   - Replace the contiguous run of those sentences with **one** sentence holding the
     rebuilt words (id = the first sentence's id, so highlight/edit ids stay stable).
   - Blank text deletes the whole run.
   - Tests: word texts/order after edit; first start & last end preserved (anchor held);
     monotonic non-decreasing starts; speaker carried; blank deletes; unknown ids no-op.

2. `joinSentences(state, sentenceIds: string[]): EditorState`
   - Merge a contiguous run into one sentence by concatenating words (no timing change).
     Used to "join sentences into a paragraph" without retyping.
   - Tests: words concatenated in order; ids collapse to first; timing untouched;
     non-contiguous ids -> no-op (or documented behaviour).

3. `splitSentence(state, sentenceId: string, wordIndex: number): EditorState`
   - Split one sentence into two at `wordIndex`; each half keeps its own words' existing
     timings (no re-spread needed). Enables paragraph split at a word boundary.
   - Tests: split point; both halves' words/timings intact; out-of-range index clamps/no-op.

(`groupParagraphs` is unchanged. Because edited paragraphs collapse to a single sentence,
re-running `groupParagraphs` still lays them out correctly; the pause between paragraphs is
preserved by each run keeping its own anchor span.)

### `app/src/Editor.tsx` changes (manually verified via `npm run tauri dev`)

- Key edit state by **paragraph index/first-sentence id** instead of sentence id.
- Replace per-sentence `onDoubleClick` with a per-paragraph one: double-click the `<p>`
  enters paragraph edit mode, seeding a `<textarea>`/contenteditable with the paragraph's
  joined text (`para.flatMap(s => s.words.map(w => w.text)).join(" ")`). Enter/blur ->
  `editParagraph(state, para.map(s => s.id), text)`; Esc cancels. (Component shape mirrors
  today's `SentenceEditor`, just at paragraph scope.)
- Keep the read mode exactly as-is (word spans, `flatCursor`, click-to-seek, highlight).
  The rAF gate stays `if (editingId === null)`.
- Add lightweight affordances (hover-revealed, between paragraphs): "Join with next"
  (-> `joinSentences` across the two paragraphs' sentence ids) and a split action on a
  word (-> `splitSentence`). These are thin dispatchers; no new logic in the view.
- No change to exports, disclaimer, transport, or the `diarized=true` branch.

### Out of scope for this PR

Word-boundary dragging on a waveform (Reduct advanced mode), forced re-alignment to audio
(Descript/otter), and a full whole-document contenteditable (Option C). Revisit C only if
users later ask for heavy rewriting rather than review-and-correct.

## Sources

- Descript: [Edit like a doc](https://help.descript.com/hc/en-us/articles/15726742913933-Edit-like-a-doc), [Realign words](https://help.descript.com/hc/en-us/articles/17506687977997-Realign-words-in-a-transcript), [The wordbar](https://help.descript.com/hc/en-us/articles/10249346632717-The-wordbar)
- Reduct: [Correct AI transcript](https://help.reduct.video/en/articles/correct-ai-transcript), [Text-based video editing](https://reduct.video/product/edit-video/)
- oTranscribe: [otranscribe.com](https://otranscribe.com/), [manual timestamps issue](https://github.com/oTranscribe/oTranscribe/issues/121)
- Subtitle Edit: [Synchronizing subtitles](https://subtitleedit.net/synchronizing-subtitles-with-subtitle-edit/)
- Aiko / MacWhisper: [Aiko](https://sindresorhus.com/aiko), [MacWhisper vs Subtitle Studio](https://subtitlestudio.ai/blog/macwhisper-vs-subtitle-studio)
- Hyprnote / Char: [Editable transcript widget](https://github.com/fastrepl/hyprnote/issues/318), [updates](https://hyprnote.com/updates/)
- otter.ai: [Edit a conversation](https://help.otter.ai/hc/en-us/articles/360047731754-Edit-a-conversation), [Interact with your transcript](https://help.otter.ai/hc/en-us/articles/360048464113-Interact-with-your-transcript)
- ProseMirror word-spans pattern: [discuss.prosemirror.net](https://discuss.prosemirror.net/t/every-word-in-a-different-span/5646)
