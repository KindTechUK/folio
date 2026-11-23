import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Chip } from "./ui";
import { ThisComputer } from "./Device";
import type { DeviceProfile } from "./Device";

// Pack trust is decided in Rust (`modelpack` crate: ed25519 + sha256 +
// engine-version gate, fail-closed). This view only lists the verdicts; a
// rejected pack is shown with its reason and never installed.

/** Mirrors the Rust `PackStatus` (serde) returned by the backend commands. */
export type PackStatus = {
  dir: string;
  id: string;
  version: string;
  engine: string;
  task: string;
  attribution: string;
  verified: boolean;
  reason: string | null;
};

export function selectAsrModel(packs: PackStatus[]): string | null {
  const ids = packs
    .filter((pack) => pack.verified && pack.task === "asr")
    .map((pack) => pack.id.toLocaleLowerCase());
  if (ids.some((id) => id.includes("parakeet"))) return "parakeet";
  if (ids.some((id) => id.includes("small"))) return "small.en";
  if (ids.some((id) => id.includes("medium"))) return "medium.en";
  return null;
}

export default function Models({
  onPacksChanged,
  device,
}: {
  onPacksChanged?: (packs: PackStatus[]) => void;
  /** This computer's tier; null while the check is still running. */
  device?: DeviceProfile | null;
}) {
  const [packs, setPacks] = useState<PackStatus[]>([]);
  const [status, setStatus] = useState<string>("");

  async function refresh() {
    try {
      const nextPacks = await invoke<PackStatus[]>("list_model_packs");
      setPacks(nextPacks);
      onPacksChanged?.(nextPacks);
    } catch (e) {
      setStatus(`error listing packs: ${String(e)}`);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function importPack() {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    setStatus(`verifying ${dir}…`);
    try {
      const result = await invoke<PackStatus>("import_model_pack", { srcDir: dir });
      setStatus(
        result.verified
          ? `imported ${result.id || result.dir} ✓`
          : `rejected — ${result.reason ?? "verification failed"}`,
      );
      await refresh();
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    }
  }

  return (
    <section>
      {device && <ThisComputer device={device} />}
      <div
        style={{
          display: "flex",
          gap: "var(--space-3)",
          alignItems: "center",
          marginBottom: "var(--space-4)",
        }}
      >
        <Button variant="primary" onClick={importPack}>
          Import model pack…
        </Button>
        {status && (
          <span role="status" style={{ color: "var(--slate-600)" }}>
            {status}
          </span>
        )}
      </div>

      {packs.length === 0 ? (
        <p style={{ color: "var(--slate-500)" }}>
          No model packs installed. Import a signed pack to add a model — unsigned or
          tampered packs are refused.
        </p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Status</th>
              <th>Model</th>
              <th>Engine / task</th>
              <th>Attribution</th>
            </tr>
          </thead>
          <tbody>
            {packs.map((p) => (
              <tr key={p.dir}>
                <td style={{ whiteSpace: "nowrap" }}>
                  {p.verified ? (
                    <Chip tone="privacy">✓ verified</Chip>
                  ) : (
                    <Chip tone="danger">✗ rejected</Chip>
                  )}
                </td>
                <td>
                  {p.id || p.dir}
                  {p.version && <span style={{ color: "var(--slate-400)" }}> v{p.version}</span>}
                  {!p.verified && p.reason && (
                    <div style={{ color: "var(--danger)", fontSize: "var(--text-xs)" }}>
                      {p.reason}
                    </div>
                  )}
                </td>
                <td>
                  {p.engine || "—"}
                  {p.task && ` / ${p.task}`}
                </td>
                <td>{p.attribution || "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
