import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SessionMeta } from "./session";

// The persisted transcript Library. Lists the saved sessions returned by the
// Rust `list_sessions` command (newest-first) and offers per-row Open/Delete.
// Like Models.tsx, this is a thin command-backed list: the Tauri `invoke`
// wiring is verified manually (not jsdom-tested); the pure logic lives in the
// tested `session.ts` / Rust `library.rs`.

export default function Library({ onOpen }: { onOpen: (id: string) => void }) {
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [status, setStatus] = useState<string>("");

  async function refresh() {
    try {
      setSessions(await invoke<SessionMeta[]>("list_sessions"));
    } catch (e) {
      setStatus(`error listing transcripts: ${String(e)}`);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function remove(id: string) {
    if (!window.confirm("Delete this saved transcript? This cannot be undone.")) {
      return;
    }
    try {
      await invoke("delete_session", { id });
      await refresh();
    } catch (e) {
      setStatus(`error deleting transcript: ${String(e)}`);
    }
  }

  return (
    <section>
      <div style={{ display: "flex", gap: 12, alignItems: "center", marginBottom: "0.75rem" }}>
        <button onClick={() => void refresh()} style={{ padding: "0.5rem 0.9rem" }}>
          Refresh
        </button>
        {status && (
          <span role="status" style={{ color: "#444" }}>
            {status}
          </span>
        )}
      </div>

      {sessions.length === 0 ? (
        <p style={{ color: "#666" }}>
          No saved transcripts yet — transcribe an audio file and it will be saved
          here automatically.
        </p>
      ) : (
        <table style={{ borderCollapse: "collapse", width: "100%" }}>
          <thead>
            <tr style={{ textAlign: "left", borderBottom: "1px solid #ddd" }}>
              <th style={{ padding: "0.4rem 0.6rem" }}>Audio</th>
              <th style={{ padding: "0.4rem 0.6rem" }}>Updated</th>
              <th style={{ padding: "0.4rem 0.6rem" }}>Segments</th>
              <th style={{ padding: "0.4rem 0.6rem" }}>Mode</th>
              <th style={{ padding: "0.4rem 0.6rem" }} />
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => (
              <tr key={s.id} style={{ borderBottom: "1px solid #f0f0f0" }}>
                <td style={{ padding: "0.4rem 0.6rem" }}>
                  {s.audioFilename}
                  {!s.audioAvailable && (
                    <div style={{ color: "#888", fontSize: "0.85em" }}>audio missing</div>
                  )}
                </td>
                <td style={{ padding: "0.4rem 0.6rem", color: "#555", whiteSpace: "nowrap" }}>
                  {new Date(s.updatedAt).toLocaleString()}
                </td>
                <td style={{ padding: "0.4rem 0.6rem", color: "#555" }}>{s.segments}</td>
                <td style={{ padding: "0.4rem 0.6rem" }}>
                  {s.diarized ? (
                    <span
                      style={{
                        background: "#e3f2fd",
                        color: "#0d47a1",
                        borderRadius: 4,
                        padding: "0.1rem 0.4rem",
                        fontSize: "0.8em",
                        fontWeight: 600,
                      }}
                    >
                      speakers
                    </span>
                  ) : (
                    <span
                      style={{
                        background: "#f0f0f0",
                        color: "#555",
                        borderRadius: 4,
                        padding: "0.1rem 0.4rem",
                        fontSize: "0.8em",
                        fontWeight: 600,
                      }}
                    >
                      paragraphs
                    </span>
                  )}
                </td>
                <td style={{ padding: "0.4rem 0.6rem", whiteSpace: "nowrap" }}>
                  <button
                    data-testid={`open-${s.id}`}
                    onClick={() => onOpen(s.id)}
                    style={{ padding: "0.35rem 0.7rem", marginRight: 8 }}
                  >
                    Open
                  </button>
                  <button
                    data-testid={`delete-${s.id}`}
                    onClick={() => void remove(s.id)}
                    style={{
                      padding: "0.35rem 0.7rem",
                      color: "#7f1d1d",
                      background: "#fee2e2",
                      border: "1px solid #b91c1c",
                      borderRadius: 6,
                      cursor: "pointer",
                    }}
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
