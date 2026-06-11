/**
 * End-to-end core harness: run a REAL engine-produced sidecar through the TS
 * `core` pipeline and assert the full chain composes on real data.
 *
 * Run via `npx tsx scripts/e2e_core.mjs` (tsx transpiles the `core/src/*.ts`
 * imports on the fly). The sidecar path comes from $E2E_SIDECAR; a human-
 * readable transcript is written to $E2E_TRANSCRIPT (default out/e2e_transcript.txt).
 *
 * ADDITIVE: this lives outside `core/test/`, so the committed vitest count
 * (69) is untouched. It imports the SAME public API the app uses.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");

const coreIndex = resolve(repoRoot, "core/src/index.ts");
const {
  buildTurns,
  loadTranscript,
  editSentence,
  renameSpeaker,
  reassignSentenceSpeaker,
  exportJSON,
  exportText,
} = await import(coreIndex);

let failures = 0;
let checks = 0;
function check(cond, msg) {
  checks++;
  if (!cond) {
    failures++;
    console.error(`  ✗ ${msg}`);
  } else {
    console.error(`  ✓ ${msg}`);
  }
}

const sidecarPath = process.env.E2E_SIDECAR;
if (!sidecarPath) {
  console.error("E2E_SIDECAR env var is required (path to engine sidecar JSON)");
  process.exit(2);
}
const transcriptOut =
  process.env.E2E_TRANSCRIPT || resolve(repoRoot, "out/e2e_transcript.txt");

const transcript = JSON.parse(readFileSync(sidecarPath, "utf8"));
check(
  Array.isArray(transcript.segments) && transcript.segments.length > 0,
  `sidecar parsed: ${transcript.segments?.length ?? 0} segments`,
);

// ---- buildTurns ----
const turns = buildTurns(transcript.segments);
check(turns.length >= 1, `buildTurns produced ${turns.length} turn(s) (>= 1)`);

let nonDecreasing = true;
for (let i = 1; i < turns.length; i++) {
  if (turns[i].start < turns[i - 1].start) nonDecreasing = false;
}
check(nonDecreasing, "turn starts are non-decreasing");

let adjacentDiffer = true;
for (let i = 1; i < turns.length; i++) {
  if (turns[i].speaker === turns[i - 1].speaker) adjacentDiffer = false;
}
check(
  turns.length === 1 || adjacentDiffer,
  "adjacent turns differ in speaker (merge invariant)",
);

let turnsWellFormed = true;
for (const t of turns) {
  if (!t.speaker || t.text.trim().length === 0 || t.end < t.start) turnsWellFormed = false;
}
check(turnsWellFormed, "every turn has a speaker, non-empty text, end >= start");

// ---- editor round-trip ----
const s0 = loadTranscript(transcript);
check(s0.sentences.length > 0, `loadTranscript -> ${s0.sentences.length} sentence(s)`);

const firstId = s0.sentences[0].id;
const origSpeaker = s0.sentences[0].words[0].speaker;
const span = {
  start: s0.sentences[0].words[0].start,
  end: s0.sentences[0].words.at(-1).end,
};

// 1) edit first sentence — words re-spread within the ORIGINAL span
const s1 = editSentence(s0, firstId, "the witness clearly stated otherwise here.");
const edited = s1.sentences.find((x) => x.id === firstId);
check(
  edited.words.map((w) => w.text).join(" ").includes("witness"),
  "editSentence: new text present",
);
check(
  Math.abs(edited.words[0].start - span.start) < 1e-3,
  `editSentence: first word start preserved (${edited.words[0].start.toFixed(3)} ~= ${span.start.toFixed(3)})`,
);
check(
  Math.abs(edited.words.at(-1).end - span.end) < 1e-3,
  `editSentence: last word end preserved (${edited.words.at(-1).end.toFixed(3)} ~= ${span.end.toFixed(3)})`,
);
let editMonotonic = true;
for (let i = 1; i < edited.words.length; i++) {
  if (edited.words[i].start < edited.words[i - 1].start) editMonotonic = false;
}
check(editMonotonic, "editSentence: re-spread word starts are non-decreasing");
check(
  !s0.sentences.find((x) => x.id === firstId).words.map((w) => w.text).join(" ").includes("witness"),
  "editSentence: input state not mutated (immutability)",
);

// 2) global rename — applies everywhere the positional id appears
const s2 = renameSpeaker(s1, origSpeaker, "Witness");
const json2 = exportJSON(s2);
check(json2.words.some((w) => w.speaker === "Witness"), "renameSpeaker: display name applied globally");
check(!json2.words.some((w) => w.speaker === origSpeaker), `renameSpeaker: no leftover ${origSpeaker}`);

// 3) per-sentence reassignment — isolated to one sentence
const targetId = s2.sentences.at(-1).id;
const beforeOthers = s2.sentences
  .filter((x) => x.id !== targetId)
  .map((x) => x.words.map((w) => w.speaker).join(","));
const s3 = reassignSentenceSpeaker(s2, targetId, "SPEAKER_99");
check(
  s3.sentences.find((x) => x.id === targetId).words.every((w) => w.speaker === "SPEAKER_99"),
  "reassignSentenceSpeaker: target sentence reassigned",
);
const afterOthers = s3.sentences
  .filter((x) => x.id !== targetId)
  .map((x) => x.words.map((w) => w.speaker).join(","));
check(
  JSON.stringify(afterOthers) === JSON.stringify(beforeOthers),
  "reassignSentenceSpeaker: other sentences untouched (isolated)",
);

// 4) exports reflect the live edits
const text = exportText(s3);
const finalJson = exportJSON(s3);
check(text.includes("Witness"), "exportText: reflects rename");
check(text.trim().length > 0, "exportText: non-empty");
check(finalJson.words.length > 0, `exportJSON: non-empty (${finalJson.words.length} words)`);

// ---- human-readable transcript for eyeballing (from the unedited turns) ----
function mmss(t) {
  const total = Math.floor(t);
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}
let out = "";
for (const t of turns) {
  out += `[${mmss(t.start)}] ${t.speaker}: ${t.text}\n\n`;
}
writeFileSync(transcriptOut, out.trimEnd() + "\n");
console.error(`\nwrote transcript: ${transcriptOut}`);

console.error(`\n${checks - failures}/${checks} core checks passed; n_turns=${turns.length}`);
if (failures > 0) {
  console.error(`FAILED: ${failures} core check(s) failed`);
  process.exit(1);
}
console.error("CORE E2E OK");
