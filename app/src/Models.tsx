import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// PR3c: the model-pack trust boundary, surfaced. Lists the packs in the packs
// dir with their VERIFIED / REJECTED status (decided in Rust by the `modelpack`
// crate — ed25519 + sha256 + engine-version gate, fail-closed), and gates
// import: an unsigned/tampered/untrusted/too-new pack is shown with its reason
// and never installed. Real audio/keys never leave the device.

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

export default function Models() {
  const [packs, setPacks] = useState<PackStatus[]>([]);
  const [status, setStatus] = useState<string>("");

  async function refresh() {
    try {
      setPacks(await invoke<PackStatus[]>("list_model_packs"));
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
      <div style={{ display: "flex", gap: 12, alignItems: "center", marginBottom: "0.75rem" }}>
        <button onClick={importPack} style={{ padding: "0.5rem 0.9rem" }}>
          Import model pack…
        </button>
        {status && (
          <span role="status" style={{ color: "#444" }}>
            {status}
          </span>
        )}
      </div>

      {packs.length === 0 ? (
        <p style={{ color: "#666" }}>
          No model packs installed. Import a signed pack to add a model — unsigned or
          tampered packs are refused.
        </p>
      ) : (
        <table style={{ borderCollapse: "collapse", width: "100%" }}>
          <thead>
            <tr style={{ textAlign: "left", borderBottom: "1px solid #ddd" }}>
              <th style={{ padding: "0.4rem 0.6rem" }}>Status</th>
              <th style={{ padding: "0.4rem 0.6rem" }}>Model</th>
              <th style={{ padding: "0.4rem 0.6rem" }}>Engine / task</th>
              <th style={{ padding: "0.4rem 0.6rem" }}>Attribution</th>
            </tr>
          </thead>
          <tbody>
            {packs.map((p) => (
              <tr key={p.dir} style={{ borderBottom: "1px solid #f0f0f0" }}>
                <td style={{ padding: "0.4rem 0.6rem", whiteSpace: "nowrap" }}>
                  {p.verified ? (
                    <span style={{ color: "#1b5e20", fontWeight: 600 }}>✓ verified</span>
                  ) : (
                    <span style={{ color: "#b71c1c", fontWeight: 600 }}>✗ rejected</span>
                  )}
                </td>
                <td style={{ padding: "0.4rem 0.6rem" }}>
                  {p.id || p.dir}
                  {p.version && <span style={{ color: "#888" }}> v{p.version}</span>}
                  {!p.verified && p.reason && (
                    <div style={{ color: "#b71c1c", fontSize: "0.85em" }}>{p.reason}</div>
                  )}
                </td>
                <td style={{ padding: "0.4rem 0.6rem", color: "#555" }}>
                  {p.engine || "—"}
                  {p.task && ` / ${p.task}`}
                </td>
                <td style={{ padding: "0.4rem 0.6rem", color: "#555" }}>{p.attribution || "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
