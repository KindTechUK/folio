import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SessionMeta } from "./session";
import { Button, Chip } from "./ui";

// The persisted transcript Library. Lists the saved sessions returned by the
// Rust `list_sessions` command (newest-first) and offers per-row Open/Delete.
// Like Models.tsx, this is a thin command-backed list: the Tauri `invoke`
// wiring is verified manually (not jsdom-tested); the pure logic lives in the
// tested `session.ts` / Rust `library.rs`.

interface LibraryProps {
  onOpen: (id: string) => void;
  onImport: () => void;
}

export default function Library({ onOpen, onImport }: LibraryProps) {
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  async function refresh() {
    try {
      setSessions(await invoke<SessionMeta[]>("list_sessions"));
      setStatus(null);
    } catch (e) {
      setStatus(`error listing transcripts: ${String(e)}`);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const visibleSessions = normalizedQuery
    ? sessions.filter((session) =>
        session.audioFilename.toLocaleLowerCase().includes(normalizedQuery),
      )
    : sessions;

  async function remove(id: string) {
    if (
      !window.confirm(
        "Remove this saved transcript from Matters? The source recording will not be deleted.",
      )
    ) {
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
          flexWrap: "wrap",
        }}
      >
        <label
          style={{
            position: "relative",
            flex: "1 1 260px",
            maxWidth: 420,
          }}
        >
          <span className="sr-only">Search matters</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search matters"
            style={{ width: "100%", padding: "8px 10px" }}
          />
        </label>
        <Button variant="ghost" onClick={() => void refresh()}>
          Refresh list
        </Button>
        <Button variant="primary" onClick={onImport} style={{ marginLeft: "auto" }}>
          Import recording
        </Button>
        {status && (
          <span
            role="status"
            style={{ color: "var(--danger)", flexBasis: "100%", fontSize: "var(--text-sm)" }}
          >
            {status}
          </span>
        )}
      </div>

      {sessions.length === 0 ? (
        <div
          style={{
            padding: "var(--space-7) var(--space-5)",
            textAlign: "center",
            background: "var(--surface)",
            border: "1px dashed var(--slate-300)",
            borderRadius: "var(--radius-lg)",
          }}
        >
          <h2 style={{ margin: 0, fontSize: "var(--text-lg)" }}>Import your first recording</h2>
          <p
            style={{
              margin: "var(--space-2) auto var(--space-4)",
              color: "var(--slate-500)",
              maxWidth: 480,
            }}
          >
            Folio transcribes audio and video entirely on this Windows device.
            The recording is never uploaded.
          </p>
          <Button variant="primary" onClick={onImport}>
            Choose a recording
          </Button>
        </div>
      ) : visibleSessions.length === 0 ? (
        <div
          style={{
            padding: "var(--space-6)",
            textAlign: "center",
            color: "var(--slate-500)",
            background: "var(--surface)",
            border: "1px solid var(--slate-200)",
            borderRadius: "var(--radius)",
          }}
        >
          No matters match “{query.trim()}”.
        </div>
      ) : (
        <table>
          <thead>
            <tr>
              <th scope="col">Matter</th>
              <th scope="col">Last updated</th>
              <th scope="col">Review setup</th>
              <th scope="col">Audio</th>
              <th scope="col" aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {visibleSessions.map((s) => (
              <tr key={s.id}>
                <td style={{ color: "var(--ink)", fontWeight: 600 }}>
                  {s.audioFilename}
                </td>
                <td className="tabular" style={{ whiteSpace: "nowrap" }}>
                  {new Date(s.updatedAt).toLocaleString()}
                </td>
                <td>
                  <Chip tone={s.diarized ? "accent" : "neutral"}>
                    {s.diarized ? "Speaker review" : "Paragraph review"}
                  </Chip>
                </td>
                <td>
                  <Chip tone={s.audioAvailable ? "privacy" : "danger"}>
                    {s.audioAvailable ? "Available" : "Missing"}
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
                    variant="ghost"
                    data-testid={`delete-${s.id}`}
                    onClick={() => void remove(s.id)}
                  >
                    Remove transcript
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
