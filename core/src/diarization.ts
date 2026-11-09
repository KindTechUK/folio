/**
 * Speaker assignment — the MAPPING layer, not the ML model.
 *
 * Mirrors whisperX's `assign_word_speakers`: for each recognized word, pick the
 * diarization segment that overlaps it most. Falls back to the nearest segment
 * when a word lands in a gap, and to {@link UNKNOWN_SPEAKER} when there are no
 * segments at all.
 *
 * Rules (deterministic):
 * - Gap distance: measured from the word's MIDPOINT to the segment, i.e.
 *   `max(0, seg.start - mid, mid - seg.end)` (0 if the midpoint is inside).
 * - Tie-break: on equal overlap (or equal nearest-distance), the segment that
 *   appears EARLIER in `speakerSegments` wins. A word exactly on the boundary
 *   between two adjacent segments therefore resolves to the earlier one.
 */
import { UNKNOWN_SPEAKER } from "./types.js";
import type { Word, SpeakerSegment } from "./types.js";

function overlap(aStart: number, aEnd: number, bStart: number, bEnd: number): number {
  return Math.max(0, Math.min(aEnd, bEnd) - Math.max(aStart, bStart));
}

function gapDistance(mid: number, seg: SpeakerSegment): number {
  return Math.max(0, seg.start - mid, mid - seg.end);
}

/**
 * Returns a NEW array of words (inputs are never mutated), each with `.speaker`
 * resolved to the best-matching segment's speaker.
 *
 * Complexity: O(words × segments).
 */
export function assignSpeakers(words: Word[], speakerSegments: SpeakerSegment[]): Word[] {
  return words.map((word) => {
    if (speakerSegments.length === 0) {
      return { ...word, speaker: UNKNOWN_SPEAKER };
    }

    let bestSpeaker = UNKNOWN_SPEAKER;
    let bestOverlap = -1;

    for (const seg of speakerSegments) {
      const ov = overlap(word.start, word.end, seg.start, seg.end);
      // Point-containment for zero-duration words: a point strictly inside a
      // segment yields zero overlap above, so treat containment as a win.
      const contained = word.start === word.end && word.start >= seg.start && word.start <= seg.end;
      const effective = contained ? Math.max(ov, Number.EPSILON) : ov;
      // Strict `>` keeps the earlier segment on ties.
      if (effective > bestOverlap) {
        bestOverlap = effective;
        bestSpeaker = seg.speaker;
      }
    }

    if (bestOverlap > 0) {
      return { ...word, speaker: bestSpeaker };
    }

    const mid = (word.start + word.end) / 2;
    let nearestSpeaker = UNKNOWN_SPEAKER;
    let nearestDist = Infinity;
    for (const seg of speakerSegments) {
      const dist = gapDistance(mid, seg);
      if (dist < nearestDist) {
        nearestDist = dist;
        nearestSpeaker = seg.speaker;
      }
    }
    return { ...word, speaker: nearestSpeaker };
  });
}
