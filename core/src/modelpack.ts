/**
 * Model-pack validation & verification — the TS reference oracle for the Rust
 * `modelpack` crate (both run the same conformance fixtures). The app verifies in Rust.
 *
 * The app FAILS CLOSED: any unsigned, tampered, malformed, or unsupported model
 * pack is rejected with a single, clear, human-readable reason (surfaced to firm
 * IT). No partial trust, no "best effort" loading.
 *
 * --- Canonicalization contract (the vendor signing pipeline MUST match) ---
 *
 * The bytes that are signed/verified are produced by {@link canonicalize}:
 *
 *   1. Take the manifest object.
 *   2. DROP the `signature` field entirely (a signature can't cover itself).
 *   3. Serialize with `JSON.stringify` over the keys SORTED in ascending
 *      Unicode-codepoint order (lexicographic byte order of the UTF-16 key
 *      strings, i.e. the default `Array.prototype.sort` on the key names), with
 *      NO whitespace and standard JSON string escaping.
 *   4. Encode that JSON text as UTF-8.
 *
 * Concretely, for the 8 manifest fields the canonical JSON is exactly:
 *   {"attribution":...,"engine":...,"engine_min_version":...,"id":...,
 *    "license":...,"sha256":...,"task":...,"version":...}
 * (keys in that sorted order, signature absent). Ed25519 signs/verifies these
 * raw UTF-8 bytes with `crypto.sign(null, ...)` / `crypto.verify(null, ...)`.
 *
 * Only the eight known manifest fields are serialized; extra keys are ignored, so
 * unvalidated data can never enter the signed bytes.
 */
import { createHash, verify as nodeVerify, createPublicKey } from "node:crypto";
import type { KeyObject } from "node:crypto";

/** Engines this build knows how to drive. */
export const ENGINES = ["whisper.cpp", "sherpa-onnx", "llama.cpp"] as const;
export type Engine = (typeof ENGINES)[number];

/** Tasks a model pack can perform. */
export const TASKS = ["asr", "diarization", "llm"] as const;
export type Task = (typeof TASKS)[number];

/**
 * A model-pack manifest, as shipped by the vendor alongside the weights.
 * `sha256` is the hex digest of the weights bytes; `signature` is the base64
 * ed25519 vendor signature over the canonical manifest (see {@link canonicalize}).
 */
export interface Manifest {
  id: string;
  version: string;
  engine: Engine;
  engine_min_version: string;
  task: Task;
  license: string;
  attribution: string;
  sha256: string;
  signature: string;
}

/** A model pack as received: an untrusted manifest plus the raw weights bytes. */
export interface ModelPack {
  manifest: unknown;
  weights: Uint8Array;
}

/** Verification context: the trusted vendor key + this build's engine versions. */
export interface VerifyOptions {
  /** Vendor ed25519 public key, as a Node KeyObject or an SPKI PEM string. */
  publicKey: KeyObject | string;
  /** Map of engine name -> the engine version bundled in this build. */
  engineVersions: Record<string, string>;
}

export type ValidateResult =
  | { ok: true; manifest: Manifest }
  | { ok: false; reason: string };

export type VerifyResult =
  | { ok: true; manifest: Manifest; attribution: string }
  | { ok: false; reason: string };

const SEMVER_RE = /^\d+\.\d+\.\d+$/;

/**
 * Numeric compare of two `x.y.z` versions. Returns <0 / 0 / >0 like a sort
 * comparator. Assumes both are valid `x.y.z` (callers validate first).
 */
export function compareSemver(a: string, b: string): number {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const da = pa[i] ?? 0;
    const db = pb[i] ?? 0;
    if (da !== db) return da - db;
  }
  return 0;
}

/** The signed fields, in sorted order. `signature` is intentionally absent. */
const CANONICAL_KEYS = [
  "attribution",
  "engine",
  "engine_min_version",
  "id",
  "license",
  "sha256",
  "task",
  "version",
] as const;

/** Canonical signed bytes — see the module-level contract. */
export function canonicalize(manifest: Omit<Manifest, "signature"> | Manifest): Uint8Array {
  const src = manifest as Record<string, unknown>;
  const ordered: Record<string, unknown> = {};
  for (const k of CANONICAL_KEYS) {
    ordered[k] = src[k];
  }
  return new TextEncoder().encode(JSON.stringify(ordered));
}

const SHA256_RE = /^[0-9a-f]{64}$/;

function isNonEmptyString(v: unknown): v is string {
  return typeof v === "string" && v.length > 0;
}

/**
 * Structurally validate an untrusted manifest object. Returns the first failure
 * as a clear, human-readable reason. Does NOT check the hash or signature — that
 * is {@link verifyModelPack}'s job.
 */
export function validateManifest(obj: unknown): ValidateResult {
  if (typeof obj !== "object" || obj === null || Array.isArray(obj)) {
    return { ok: false, reason: "manifest is not a JSON object" };
  }
  const m = obj as Record<string, unknown>;

  // Required non-empty string fields (presence + type). `signature` is handled
  // separately: it must be PRESENT and a string, but an EMPTY signature is a
  // valid (if useless) shape that fails closed later as "unsigned" at the
  // signature-verification step — not a structural error.
  for (const field of ["id", "version", "engine_min_version", "license", "attribution", "sha256"]) {
    if (!(field in m)) {
      return { ok: false, reason: `manifest is missing required field "${field}"` };
    }
    if (!isNonEmptyString(m[field])) {
      return { ok: false, reason: `manifest field "${field}" must be a non-empty string` };
    }
  }
  if (!("signature" in m)) {
    return { ok: false, reason: `manifest is missing required field "signature"` };
  }
  if (typeof m.signature !== "string") {
    return { ok: false, reason: `manifest field "signature" must be a string` };
  }

  if (!("engine" in m) || !isNonEmptyString(m.engine)) {
    return { ok: false, reason: `manifest is missing required field "engine"` };
  }
  if (!(ENGINES as readonly string[]).includes(m.engine)) {
    return {
      ok: false,
      reason: `manifest engine "${m.engine}" is not one of: ${ENGINES.join(", ")}`,
    };
  }

  if (!("task" in m) || !isNonEmptyString(m.task)) {
    return { ok: false, reason: `manifest is missing required field "task"` };
  }
  if (!(TASKS as readonly string[]).includes(m.task)) {
    return {
      ok: false,
      reason: `manifest task "${m.task}" is not one of: ${TASKS.join(", ")}`,
    };
  }
  // A language model runs only on llama.cpp, and llama.cpp runs nothing else,
  // so a pack can't pass the engine check for one runtime and be loaded by another.
  if ((m.task === "llm") !== (m.engine === "llama.cpp")) {
    return { ok: false, reason: `manifest task "${m.task}" cannot run on engine "${m.engine}"` };
  }

  if (!SEMVER_RE.test(m.version as string)) {
    return { ok: false, reason: `manifest version "${m.version as string}" is not valid semver (expected x.y.z)` };
  }
  if (!SEMVER_RE.test(m.engine_min_version as string)) {
    return {
      ok: false,
      reason: `manifest engine_min_version "${m.engine_min_version as string}" is not valid semver (expected x.y.z)`,
    };
  }

  if (!SHA256_RE.test(m.sha256 as string)) {
    return { ok: false, reason: `manifest sha256 must be 64 lowercase hex characters` };
  }

  return { ok: true, manifest: m as unknown as Manifest };
}

function toPublicKey(key: KeyObject | string): KeyObject {
  return typeof key === "string" ? createPublicKey(key) : key;
}

/**
 * Fully verify a model pack, failing closed at the first problem. Steps, IN ORDER:
 *   1. {@link validateManifest} passes;
 *   2. sha256(weights) hex === manifest.sha256 (constant-time compare);
 *   3. the base64 signature is a valid vendor ed25519 signature over the
 *      canonical manifest (signature field excluded);
 *   4. manifest.engine_min_version <= this build's engine version.
 */
export function verifyModelPack(pack: ModelPack, opts: VerifyOptions): VerifyResult {
  const v = validateManifest(pack.manifest);
  if (!v.ok) return { ok: false, reason: v.reason };
  const manifest = v.manifest;

  const actual = createHash("sha256").update(pack.weights).digest("hex");
  if (!timingSafeHexEqual(actual, manifest.sha256)) {
    return { ok: false, reason: "weights hash mismatch (tampered or corrupt)" };
  }

  let signatureOk = false;
  try {
    const sig = Buffer.from(manifest.signature, "base64");
    // An empty (or entirely non-base64) signature fails closed without calling verify.
    if (sig.length > 0) {
      const data = canonicalize(manifest);
      signatureOk = nodeVerify(null, data, toPublicKey(opts.publicKey), sig);
    }
  } catch {
    signatureOk = false;
  }
  if (!signatureOk) {
    return { ok: false, reason: "signature invalid (unsigned or untrusted pack)" };
  }

  const available = opts.engineVersions[manifest.engine];
  if (available === undefined) {
    return { ok: false, reason: `engine not supported by this build (no "${manifest.engine}" engine available)` };
  }
  if (!SEMVER_RE.test(available)) {
    return { ok: false, reason: `installed engine version "${available}" for "${manifest.engine}" is not valid semver` };
  }
  if (compareSemver(manifest.engine_min_version, available) > 0) {
    return {
      ok: false,
      reason:
        `model pack requires ${manifest.engine} >= ${manifest.engine_min_version}, ` +
        `but this build has ${available}; update the app to load this pack`,
    };
  }

  return { ok: true, manifest, attribution: manifest.attribution };
}

/**
 * Constant-time equality for two hex digest strings of equal expected length.
 * Avoids leaking how many leading bytes matched to a timing observer.
 */
function timingSafeHexEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}
