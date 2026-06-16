import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SessionMeta } from "./session";
import { Button, Chip } from "./ui";

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
      <div
        style={{
          display: "flex",
          gap: "var(--space-3)",
          alignItems: "center",
          marginBottom: "var(--space-4)",
        }}
      >
        <Button onClick={() => void refresh()}>Refresh</Button>
        {status && (
          <span role="status" style={{ color: "var(--slate-600)" }}>
            {status}
          </span>
        )}
      </div>

      {sessions.length === 0 ? (
        <p style={{ color: "var(--slate-500)" }}>
          No saved transcripts yet — transcribe an audio file and it will be saved
          here automatically.
        </p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Audio</th>
              <th>Updated</th>
              <th>Segments</th>
              <th>Mode</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => (
              <tr key={s.id}>
                <td>
                  {s.audioFilename}
                  {!s.audioAvailable && (
                    <div style={{ color: "var(--slate-400)", fontSize: "var(--text-xs)" }}>
                      audio missing
                    </div>
                  )}
                </td>
                <td className="tabular" style={{ whiteSpace: "nowrap" }}>
                  {new Date(s.updatedAt).toLocaleString()}
                </td>
                <td>{s.segments}</td>
                <td>
                  <Chip tone={s.diarized ? "accent" : "neutral"}>
                    {s.diarized ? "speakers" : "paragraphs"}
                  </Chip>
                </td>
                <td style={{ whiteSpace: "nowrap" }}>
                  <Button
                    size="sm"
                    data-testid={`open-${s.id}`}
                    onClick={() => onOpen(s.id)}
                    style={{ marginRight: 8 }}
                  >
                    Open
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    data-testid={`delete-${s.id}`}
                    onClick={() => void remove(s.id)}
                  >
                    Delete
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
