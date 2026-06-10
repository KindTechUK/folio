import { describe, it, expect } from "vitest";
import {
  createPublicKey,
  generateKeyPairSync,
  sign as nodeSign,
  type KeyObject,
} from "node:crypto";
import {
  validateManifest,
  verifyModelPack,
  compareSemver,
  canonicalize,
} from "../src/modelpack.js";
import type { Manifest } from "../src/modelpack.js";

// --- helpers -------------------------------------------------------------

function makeKeypair(): { publicKey: KeyObject; privateKey: KeyObject } {
  return generateKeyPairSync("ed25519");
}

/** Sign a manifest's canonical bytes (signature field excluded) with a private key. */
function signManifest(manifest: Omit<Manifest, "signature">, privateKey: KeyObject): string {
  const bytes = canonicalize(manifest);
  return nodeSign(null, bytes, privateKey).toString("base64");
}

const baseUnsigned: Omit<Manifest, "signature"> = {
  id: "whisper-medium-en",
  version: "1.2.0",
  engine: "whisper.cpp",
  engine_min_version: "1.7.0",
  task: "asr",
  license: "MIT",
  attribution: "OpenAI Whisper",
  // sha256 filled per-test from the actual weights
  sha256: "",
};

const WEIGHTS = new Uint8Array([0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33]);

// real sha256 of WEIGHTS, computed independently in the test for honesty
import { createHash } from "node:crypto";
function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function buildSignedPack(privateKey: KeyObject) {
  const unsigned: Omit<Manifest, "signature"> = {
    ...baseUnsigned,
    sha256: sha256Hex(WEIGHTS),
  };
  const signature = signManifest(unsigned, privateKey);
  const manifest: Manifest = { ...unsigned, signature };
  return { manifest, weights: WEIGHTS.slice() };
}

// --- compareSemver -------------------------------------------------------

describe("compareSemver", () => {
  it("orders by numeric parts, not lexically", () => {
    expect(compareSemver("1.10.0", "1.9.0")).toBeGreaterThan(0);
  });
  it("treats equal versions as 0", () => {
    expect(compareSemver("1.2.0", "1.2.0")).toBe(0);
  });
  it("major dominates minor/patch", () => {
    expect(compareSemver("2.0.0", "1.99.99")).toBeGreaterThan(0);
  });
});

// --- validateManifest ----------------------------------------------------

describe("validateManifest", () => {
  it("accepts a well-formed manifest", () => {
    const m = { ...baseUnsigned, sha256: sha256Hex(WEIGHTS), signature: "AA==" };
    const r = validateManifest(m);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.manifest.id).toBe("whisper-medium-en");
  });

  it("rejects a missing required field with a clear reason", () => {
    const m: Record<string, unknown> = { ...baseUnsigned, sha256: "a".repeat(64), signature: "AA==" };
    delete m.license;
    const r = validateManifest(m);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason.toLowerCase()).toContain("license");
  });

  it("rejects bad semver version", () => {
    const m = { ...baseUnsigned, version: "1.2", sha256: "a".repeat(64), signature: "AA==" };
    const r = validateManifest(m);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason.toLowerCase()).toContain("version");
  });

  it("rejects bad sha256 length", () => {
    const m = { ...baseUnsigned, sha256: "abc", signature: "AA==" };
    const r = validateManifest(m);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason.toLowerCase()).toContain("sha256");
  });

  it("rejects non-hex sha256 of correct length", () => {
    const m = { ...baseUnsigned, sha256: "z".repeat(64), signature: "AA==" };
    const r = validateManifest(m);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason.toLowerCase()).toContain("sha256");
  });

  it("rejects an unknown engine", () => {
    const m = { ...baseUnsigned, engine: "ggml-magic", sha256: "a".repeat(64), signature: "AA==" };
    const r = validateManifest(m);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason.toLowerCase()).toContain("engine");
  });

  it("rejects an unknown task", () => {
    const m = { ...baseUnsigned, task: "translation", sha256: "a".repeat(64), signature: "AA==" };
    const r = validateManifest(m);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason.toLowerCase()).toContain("task");
  });

  it("rejects a non-object input", () => {
    const r = validateManifest(null);
    expect(r.ok).toBe(false);
  });
});

// --- verifyModelPack -----------------------------------------------------

describe("verifyModelPack", () => {
  const engineVersions = { "whisper.cpp": "1.7.5", "sherpa-onnx": "1.10.0" };

  it("accepts a correctly-signed pack with matching sha256 and surfaces attribution", () => {
    const { publicKey, privateKey } = makeKeypair();
    const pack = buildSignedPack(privateKey);
    const r = verifyModelPack(pack, { publicKey, engineVersions });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.attribution).toBe("OpenAI Whisper");
      expect(r.manifest.id).toBe("whisper-medium-en");
    }
  });

  it("accepts a PEM-string public key too", () => {
    const { publicKey, privateKey } = makeKeypair();
    const pack = buildSignedPack(privateKey);
    const pem = publicKey.export({ type: "spki", format: "pem" }).toString();
    const r = verifyModelPack(pack, { publicKey: pem, engineVersions });
    expect(r.ok).toBe(true);
  });

  it("rejects tampered weights (flip a byte) with hash-mismatch reason", () => {
    const { publicKey, privateKey } = makeKeypair();
    const pack = buildSignedPack(privateKey);
    const tampered = pack.weights.slice();
    tampered[0] = tampered[0]! ^ 0xff; // flip a byte on a COPY
    const r = verifyModelPack({ manifest: pack.manifest, weights: tampered }, { publicKey, engineVersions });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toContain("weights hash mismatch (tampered or corrupt)");
  });

  it("rejects an unsigned pack (garbage signature) with the signature reason", () => {
    const { publicKey, privateKey } = makeKeypair();
    const pack = buildSignedPack(privateKey);
    const bad = { ...pack.manifest, signature: "" };
    const r = verifyModelPack({ manifest: bad, weights: pack.weights }, { publicKey, engineVersions });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toContain("signature invalid (unsigned or untrusted pack)");
  });

  it("rejects a pack signed by a DIFFERENT (untrusted) key", () => {
    const { privateKey } = makeKeypair();
    const { publicKey: otherPub } = makeKeypair();
    const pack = buildSignedPack(privateKey);
    const r = verifyModelPack(pack, { publicKey: otherPub, engineVersions });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toContain("signature invalid (unsigned or untrusted pack)");
  });

  it("rejects a manifest whose content was altered after signing (signature covers content)", () => {
    const { publicKey, privateKey } = makeKeypair();
    const pack = buildSignedPack(privateKey);
    // keep the valid signature but change a signed field on a COPY; sha256 must still
    // match the weights so we reach the signature step, so tamper a non-hash field.
    const altered: Manifest = { ...pack.manifest, attribution: "Evil Corp" };
    const r = verifyModelPack({ manifest: altered, weights: pack.weights }, { publicKey, engineVersions });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toContain("signature invalid (unsigned or untrusted pack)");
  });

  it("refuses when engine_min_version is newer than the app's engine version", () => {
    const { publicKey, privateKey } = makeKeypair();
    const unsigned: Omit<Manifest, "signature"> = {
      ...baseUnsigned,
      engine_min_version: "1.9.0",
      sha256: sha256Hex(WEIGHTS),
    };
    const manifest: Manifest = { ...unsigned, signature: signManifest(unsigned, privateKey) };
    const r = verifyModelPack(
      { manifest, weights: WEIGHTS.slice() },
      { publicKey, engineVersions: { "whisper.cpp": "1.7.5" } },
    );
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.reason).toContain("1.9.0"); // required
      expect(r.reason).toContain("1.7.5"); // available
    }
  });

  it("rejects an engine not present in engineVersions", () => {
    const { publicKey, privateKey } = makeKeypair();
    const unsigned: Omit<Manifest, "signature"> = {
      ...baseUnsigned,
      engine: "sherpa-onnx",
      sha256: sha256Hex(WEIGHTS),
    };
    const manifest: Manifest = { ...unsigned, signature: signManifest(unsigned, privateKey) };
    const r = verifyModelPack(
      { manifest, weights: WEIGHTS.slice() },
      { publicKey, engineVersions: { "whisper.cpp": "1.7.5" } },
    );
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toContain("engine not supported by this build");
  });

  it("fails validation before hashing if the manifest is malformed", () => {
    const { publicKey } = makeKeypair();
    const r = verifyModelPack(
      { manifest: { junk: true }, weights: WEIGHTS.slice() },
      { publicKey, engineVersions },
    );
    expect(r.ok).toBe(false);
  });

  it("accepts when engine_min_version equals available version (<=)", () => {
    const { publicKey, privateKey } = makeKeypair();
    const unsigned: Omit<Manifest, "signature"> = {
      ...baseUnsigned,
      engine_min_version: "1.7.5",
      sha256: sha256Hex(WEIGHTS),
    };
    const manifest: Manifest = { ...unsigned, signature: signManifest(unsigned, privateKey) };
    const r = verifyModelPack(
      { manifest, weights: WEIGHTS.slice() },
      { publicKey, engineVersions: { "whisper.cpp": "1.7.5" } },
    );
    expect(r.ok).toBe(true);
  });
});

// --- canonicalize contract ----------------------------------------------

describe("canonicalize", () => {
  it("is deterministic regardless of key insertion order and excludes signature", () => {
    const a = canonicalize({
      id: "x", version: "1.0.0", engine: "whisper.cpp", engine_min_version: "1.0.0",
      task: "asr", license: "MIT", attribution: "A", sha256: "a".repeat(64),
    });
    const b = canonicalize({
      sha256: "a".repeat(64), attribution: "A", license: "MIT", task: "asr",
      engine_min_version: "1.0.0", engine: "whisper.cpp", version: "1.0.0", id: "x",
      signature: "should-be-ignored",
    } as Omit<Manifest, "signature">);
    expect(Buffer.from(a).toString("utf8")).toBe(Buffer.from(b).toString("utf8"));
    expect(Buffer.from(a).toString("utf8")).not.toContain("signature");
  });
});

void createPublicKey;
