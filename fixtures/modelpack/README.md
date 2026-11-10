# Model-pack cross-language conformance fixtures

These fixtures prove that the two model-pack verifiers — the TypeScript oracle
(`core/src/modelpack.ts`) and the Rust trust-boundary port (`modelpack/`) — make
**byte-for-byte-identical trust decisions** on identical inputs.

They are consumed by two tests that load the SAME `cases/*`:

- `core/test/modelpack.conformance.test.ts` (TS oracle)
- `modelpack/tests/conformance.rs` (Rust port)

## ⚠️ The keys here are TEST-ONLY — NOT production material

`keys/` holds two ed25519 keypairs generated solely for these fixtures:

- `test-*` — the "trusted vendor" key the conformance suite verifies against.
- `untrusted-*` — a second key used only to produce the `untrusted` reject case.

Each keypair is provided as:

- `*-private.pkcs8.pem` — PKCS#8 private key (used by the generator to sign).
- `*-public.spki.pem` — SPKI PEM public key.
- `*-public.raw.hex` — the raw 32-byte ed25519 public key, hex-encoded.

**These private keys are committed deliberately and are worthless: they sign
nothing real.** The real vendor signing key is held as a CI secret and release packs
are produced by `tools/modelpack-sign`; release app builds refuse to compile against
the `test` key. Never load a production model pack against these keys.

## Regenerating (deterministic)

```sh
core/node_modules/.bin/vite-node scripts/gen-modelpack-fixtures.ts
```

The generator imports `canonicalize` from the TS oracle and signs with the
committed `test` private key. It is fully deterministic — re-running produces no
git diff (keys are read from disk, never regenerated; all manifest/weights bytes
are fixed in the script). It rewrites `cases/` and `context.json` only.

## Layout

- `context.json` — the trusted pubkey (SPKI PEM + raw hex) and the
  `engineVersions` map both conformance tests use.
- `cases/<name>/manifest.json` — the full manifest (including `signature`).
- `cases/<name>/weights.bin` — the weights bytes (omitted for malformed-only
  cases, which are rejected at structural validation before any hash step).
- `cases/<name>/expected.json` — `{ "outcome": "ok"|"reject", "reasonContains"?: string }`.

## Cases

| case | outcome | proves |
| --- | --- | --- |
| `valid` | ok | canonicalization + ed25519 parity (the decisive cross-impl check) |
| `tampered-weights` | reject | sha256 mismatch detected |
| `unsigned` | reject | empty signature fails closed |
| `untrusted` | reject | signature by a different key rejected |
| `tampered-manifest` | reject | valid sig but a field mutated post-sign |
| `engine-too-new` | reject | `engine_min_version` > installed engine version |
| `malformed-missing-field` | reject | missing required field |
| `malformed-bad-semver` | reject | non-`x.y.z` version |
| `malformed-bad-sha256` | reject | sha256 not 64 lowercase hex |
| `malformed-uppercase-sha256` | reject | uppercase hex sha256 rejected (lowercase only) |
| `malformed-unknown-engine` | reject | engine not in the allowed set |
| `malformed-unknown-task` | reject | task not in the allowed set |
