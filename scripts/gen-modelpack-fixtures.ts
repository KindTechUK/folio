/**
 * gen-modelpack-fixtures.ts — generator for the cross-language model-pack
 * conformance fixtures.
 *
 * Run with vite-node (resolves core's TS imports without a build step):
 *
 *   core/node_modules/.bin/vite-node scripts/gen-modelpack-fixtures.ts
 *
 * It imports `canonicalize` from the TS oracle (`core/src/modelpack.ts`) and signs
 * manifests with the committed TEST-ONLY ed25519 key (node crypto
 * `sign(null, canonicalize(manifest), key)` — PureEdDSA, matching the Rust
 * `ed25519-dalek` verify path). Output is fully deterministic: re-running it
 * produces NO git diff (the keypairs are read from disk, not regenerated, and
 * every manifest/weights byte is fixed in this file).
 *
 * Emits, under fixtures/modelpack/:
 *   context.json                       — pubkey (SPKI PEM + raw hex) + engineVersions
 *   cases/<name>/manifest.json         — full manifest incl. `signature`
 *   cases/<name>/weights.bin           — weights bytes (where relevant)
 *   cases/<name>/expected.json         — { outcome: "ok"|"reject", reasonContains?: string }
 *
 * The keys under fixtures/modelpack/keys/ are EXPLICITLY NON-PRODUCTION (see
 * fixtures/modelpack/README.md).
 */
import { createHash, createPrivateKey, sign as nodeSign } from "node:crypto";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from "node:fs";

// `canonicalize` is not re-exported from core's index, so import the module directly.
import { canonicalize } from "../core/src/modelpack.js";
import type { Manifest } from "../core/src/modelpack.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const fixturesDir = resolve(repoRoot, "fixtures/modelpack");
const casesDir = resolve(fixturesDir, "cases");
const keysDir = resolve(fixturesDir, "keys");

// --- keys (read committed test-only material; never regenerated here) ------
const testPrivPem = readFileSync(resolve(keysDir, "test-private.pkcs8.pem"), "utf8");
const testPubSpkiPem = readFileSync(resolve(keysDir, "test-public.spki.pem"), "utf8");
const testPubRawHex = readFileSync(resolve(keysDir, "test-public.raw.hex"), "utf8").trim();
const untrustedPrivPem = readFileSync(resolve(keysDir, "untrusted-private.pkcs8.pem"), "utf8");

const testPrivKey = createPrivateKey(testPrivPem);
const untrustedPrivKey = createPrivateKey(untrustedPrivPem);

// The engine versions this "build" provides (recorded in context.json so both
// language conformance tests use the SAME map).
const ENGINE_VERSIONS = {
  "whisper.cpp": "1.7.0",
  "sherpa-onnx": "1.13.2",
  "llama.cpp": "0.6.0",
};

// --- helpers ---------------------------------------------------------------

type Expected = { outcome: "ok" | "reject"; reasonContains?: string };

function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

/** Sign the canonical bytes of a manifest (signature field excluded). */
function signWith(key: ReturnType<typeof createPrivateKey>, manifest: Omit<Manifest, "signature">): string {
  const data = canonicalize(manifest);
  return nodeSign(null, Buffer.from(data), key).toString("base64");
}

function writeCase(
  name: string,
  manifest: Record<string, unknown>,
  expected: Expected,
  weights?: Uint8Array,
): void {
  const dir = resolve(casesDir, name);
  mkdirSync(dir, { recursive: true });
  // Pretty-print the manifest deterministically (2-space) so the committed
  // fixture is human-reviewable; the conformance tests parse it back to bytes
  // for canonicalization, so on-disk formatting is irrelevant to the crypto.
  writeFileSync(resolve(dir, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  writeFileSync(resolve(dir, "expected.json"), JSON.stringify(expected, null, 2) + "\n");
  if (weights !== undefined) {
    writeFileSync(resolve(dir, "weights.bin"), Buffer.from(weights));
  }
}

// Deterministic "weights" — a fixed byte pattern (NOT random).
function makeWeights(seed: number, len = 256): Uint8Array {
  const out = new Uint8Array(len);
  for (let i = 0; i < len; i++) out[i] = (seed + i * 31) & 0xff;
  return out;
}

// --- start clean so re-runs are byte-identical -----------------------------
if (existsSync(casesDir)) rmSync(casesDir, { recursive: true, force: true });
mkdirSync(casesDir, { recursive: true });

// --- context.json ----------------------------------------------------------
writeFileSync(
  resolve(fixturesDir, "context.json"),
  JSON.stringify(
    { pubkeySpkiPem: testPubSpkiPem, pubkeyRawHex: testPubRawHex, engineVersions: ENGINE_VERSIONS },
    null,
    2,
  ) + "\n",
);

// A well-formed base manifest (without signature). All cases derive from this.
function baseManifest(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: "acme/whisper-base-en",
    version: "1.2.3",
    engine: "whisper.cpp",
    engine_min_version: "1.5.0",
    task: "asr",
    license: "MIT",
    attribution: "Acme Speech Labs",
    sha256: "", // filled per-case from the weights
    ...overrides,
  };
}

// === ok: valid =============================================================
{
  const weights = makeWeights(1);
  const m = baseManifest({ sha256: sha256Hex(weights) });
  const signature = signWith(testPrivKey, m as Omit<Manifest, "signature">);
  writeCase("valid", { ...m, signature }, { outcome: "ok" }, weights);
}

// === ok: valid-llm (task "llm" on the llama.cpp engine) ===================
{
  const weights = makeWeights(7);
  const m = baseManifest({
    id: "acme/qwen3-4b-instruct",
    engine: "llama.cpp",
    engine_min_version: "0.6.0",
    task: "llm",
    license: "Apache-2.0",
    attribution: "Acme Language Labs",
    sha256: sha256Hex(weights),
  });
  const signature = signWith(testPrivKey, m as Omit<Manifest, "signature">);
  writeCase("valid-llm", { ...m, signature }, { outcome: "ok" }, weights);
}

// === reject: tampered-weights (flip a byte after signing) ==================
{
  const weights = makeWeights(2);
  const m = baseManifest({ sha256: sha256Hex(weights) });
  const signature = signWith(testPrivKey, m as Omit<Manifest, "signature">);
  const tampered = Uint8Array.from(weights);
  tampered[0] ^= 0x01; // flip a bit -> hash no longer matches manifest.sha256
  writeCase(
    "tampered-weights",
    { ...m, signature },
    { outcome: "reject", reasonContains: "weights hash mismatch" },
    tampered,
  );
}

// === reject: unsigned (empty signature) ====================================
{
  const weights = makeWeights(3);
  const m = baseManifest({ sha256: sha256Hex(weights) });
  writeCase(
    "unsigned",
    { ...m, signature: "" },
    { outcome: "reject", reasonContains: "unsigned or untrusted" },
    weights,
  );
}

// === reject: untrusted (signed by a DIFFERENT key) =========================
{
  const weights = makeWeights(4);
  const m = baseManifest({ sha256: sha256Hex(weights) });
  const signature = signWith(untrustedPrivKey, m as Omit<Manifest, "signature">);
  writeCase(
    "untrusted",
    { ...m, signature },
    { outcome: "reject", reasonContains: "unsigned or untrusted" },
    weights,
  );
}

// === reject: tampered-manifest (valid sig, but a field mutated post-sign) ==
{
  const weights = makeWeights(5);
  const m = baseManifest({ sha256: sha256Hex(weights) });
  const signature = signWith(testPrivKey, m as Omit<Manifest, "signature">);
  // Mutate attribution AFTER signing: signature no longer covers canonical bytes.
  const mutated = { ...m, attribution: "Totally Different Vendor", signature };
  writeCase(
    "tampered-manifest",
    mutated,
    { outcome: "reject", reasonContains: "unsigned or untrusted" },
    weights,
  );
}

// === reject: engine-too-new (engine_min_version > installed) ===============
{
  const weights = makeWeights(6);
  const m = baseManifest({ engine_min_version: "9.9.9", sha256: sha256Hex(weights) });
  const signature = signWith(testPrivKey, m as Omit<Manifest, "signature">);
  writeCase(
    "engine-too-new",
    { ...m, signature },
    { outcome: "reject", reasonContains: "update the app" },
    weights,
  );
}

// === reject: malformed-missing-field (no weights needed) ===================
{
  const m = baseManifest({ sha256: "a".repeat(64) });
  delete (m as Record<string, unknown>).license;
  writeCase(
    "malformed-missing-field",
    { ...m, signature: "AA==" },
    { outcome: "reject", reasonContains: 'missing required field "license"' },
  );
}

// === reject: malformed-bad-semver ==========================================
{
  const m = baseManifest({ version: "1.2", sha256: "a".repeat(64) });
  writeCase(
    "malformed-bad-semver",
    { ...m, signature: "AA==" },
    { outcome: "reject", reasonContains: "is not valid semver" },
  );
}

// === reject: malformed-bad-sha256 ==========================================
{
  const m = baseManifest({ sha256: "not-a-valid-sha" });
  writeCase(
    "malformed-bad-sha256",
    { ...m, signature: "AA==" },
    { outcome: "reject", reasonContains: "64 lowercase hex" },
  );
}

// === reject: malformed-uppercase-sha256 ====================================
{
  const m = baseManifest({ sha256: "A".repeat(64) });
  writeCase(
    "malformed-uppercase-sha256",
    { ...m, signature: "AA==" },
    { outcome: "reject", reasonContains: "64 lowercase hex" },
  );
}

// === reject: malformed-unknown-engine ======================================
{
  const m = baseManifest({ engine: "kaldi", sha256: "a".repeat(64) });
  writeCase(
    "malformed-unknown-engine",
    { ...m, signature: "AA==" },
    { outcome: "reject", reasonContains: "is not one of" },
  );
}

// === reject: malformed-unknown-task ========================================
{
  const m = baseManifest({ task: "translation", sha256: "a".repeat(64) });
  writeCase(
    "malformed-unknown-task",
    { ...m, signature: "AA==" },
    { outcome: "reject", reasonContains: "is not one of" },
  );
}

console.error("model-pack fixtures written to fixtures/modelpack/");
