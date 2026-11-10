import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Transcript } from "@core/types";
import { loadTranscript } from "@core/editor";

const invokeMock = vi.fn();
const openMock = vi.fn();
const listenMock = vi.fn();
let deleteMatterReport: { deleted: string[]; failed: unknown[] };
let pickRecording: () => Promise<unknown>;
let saveSession: () => Promise<unknown>;
let transcribe: () => Promise<unknown>;
let sessions: unknown[];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  convertFileSrc: (path: string) => `asset://${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
}));

import App from "./App";

const transcript: Transcript = {
  segments: [
    {
      start: 0,
      end: 2,
      text: "The local transcript is ready.",
      words: [
        { word: "The", start: 0, end: 0.2 },
        { word: "local", start: 0.2, end: 0.5 },
        { word: "transcript", start: 0.5, end: 1 },
        { word: "is", start: 1, end: 1.2 },
        { word: "ready.", start: 1.2, end: 2 },
      ],
    },
  ],
};

beforeEach(() => {
  invokeMock.mockReset();
  openMock.mockReset();
  listenMock.mockReset();
  openMock.mockResolvedValue("/recordings/interview.wav");
  listenMock.mockResolvedValue(() => {});
  deleteMatterReport = { deleted: ["/recordings/interview.wav"], failed: [] };
  pickRecording = () => Promise.resolve("/recordings/interview.wav");
  saveSession = () => Promise.resolve();
  transcribe = () => Promise.resolve(transcript);
  sessions = [];
  invokeMock.mockImplementation((command: string) => {
    switch (command) {
      case "volume_encryption_status":
        return Promise.resolve({ state: "encrypted", detail: "test volume" });
      case "list_model_packs":
        return Promise.resolve([
          {
            dir: "asr-small-en",
            id: "asr-whisper-small.en",
            version: "1.0.0",
            engine: "sherpa-onnx",
            task: "asr",
            attribution: "OpenAI Whisper",
            verified: true,
            reason: null,
          },
        ]);
      case "debug_asr_model_override":
        return Promise.resolve(null);
      case "list_sessions":
        return Promise.resolve(sessions);
      case "load_session":
        return Promise.resolve({ session: SECOND_SESSION, audioAvailable: true });
      case "pick_recording":
        return pickRecording();
      case "transcribe":
        return transcribe();
      case "save_session":
        return saveSession();
      case "delete_matter":
        return Promise.resolve(deleteMatterReport);
      case "delete_session":
        return Promise.resolve();
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
});

async function importAndCreate(): Promise<void> {
  await screen.findByText("Import your first recording");
  fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
  fireEvent.click(await screen.findByRole("button", { name: "Choose recording" }));
  fireEvent.click(await screen.findByRole("button", { name: "Create transcript" }));
  await screen.findByTestId("paragraph");
}

const SECOND_SESSION = {
  id: "second",
  createdAt: "2026-11-01T09:00:00Z",
  updatedAt: "2026-11-01T09:00:00Z",
  audioPath: "/recordings/second.wav",
  audioFilename: "second.wav",
  diarized: false,
  segments: 1,
  editorState: loadTranscript({
    segments: [
      {
        start: 0,
        end: 1,
        text: "Second recording.",
        words: [
          { word: "Second", start: 0, end: 0.5 },
          { word: "recording.", start: 0.5, end: 1 },
        ],
      },
    ],
  }),
};

function savedTexts(): string[] {
  return invokeMock.mock.calls
    .filter(([command]) => command === "save_session")
    .map(([, args]) =>
      (args as { session: { editorState: { sentences: { words: { text: string }[] }[] } } })
        .session.editorState.sentences.flatMap((s) => s.words.map((w) => w.text))
        .join(" "),
    );
}

function correctTo(text: string): void {
  fireEvent.doubleClick(screen.getByTestId("paragraph"));
  const editor = screen.getByTestId("edit-sent-0");
  fireEvent.change(editor, { target: { value: text } });
  fireEvent.blur(editor);
}

describe("App import flow", () => {
  it("opens a recording, transcribes it with the installed model, and renders the editor", async () => {
    render(<App />);
    await screen.findByText("Import your first recording");

    fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
    const choose = await screen.findByRole("button", { name: "Choose recording" });
    fireEvent.click(choose);
    fireEvent.click(await screen.findByRole("button", { name: "Create transcript" }));

    expect(await screen.findByTestId("paragraph")).toHaveTextContent(
      "The local transcript is ready.",
    );
    expect(invokeMock).toHaveBeenCalledWith("pick_recording");
    expect(openMock).not.toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("transcribe", {
      path: "/recordings/interview.wav",
      diarize: false,
      num_speakers: undefined,
      model: "small.en",
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "save_session",
        expect.objectContaining({ session: expect.any(Object) }),
      ),
    );
  });

  it("deletes transcript audio by backend-owned session id, not a renderer path", async () => {
    render(<App />);
    await importAndCreate();
    const saveCall = invokeMock.mock.calls.find(([command]) => command === "save_session");
    const sessionId = saveCall?.[1]?.session?.id;
    expect(sessionId).toEqual(expect.any(String));

    fireEvent.click(screen.getByTestId("delete-transcript"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_matter", { id: sessionId }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "delete_matter",
      expect.objectContaining({ paths: expect.anything() }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("delete_session", expect.anything());
  });

  it("keeps the transcript when source-audio deletion reports a failure", async () => {
    deleteMatterReport = {
      deleted: [],
      failed: [{ path: "/recordings/interview.wav", error: "locked" }],
    };

    render(<App />);
    await importAndCreate();
    fireEvent.click(screen.getByTestId("delete-transcript"));

    expect(await screen.findByText(/source audio could not be deleted/i)).toBeInTheDocument();
    expect(screen.getByTestId("paragraph")).toHaveTextContent("The local transcript is ready.");
    expect(invokeMock).not.toHaveBeenCalledWith("delete_session", expect.anything());
  });

  it("reports backend picker errors instead of leaving an unhandled rejection", async () => {
    pickRecording = () => Promise.reject(new Error("registry unavailable"));

    render(<App />);
    await screen.findByText("Import your first recording");
    fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
    fireEvent.click(await screen.findByRole("button", { name: "Choose recording" }));

    expect(await screen.findByText(/registry unavailable/i)).toBeInTheDocument();
  });

  it("waits for Create transcript before transcribing the chosen recording", async () => {
    render(<App />);
    await screen.findByText("Import your first recording");
    fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
    fireEvent.click(await screen.findByRole("button", { name: "Choose recording" }));

    expect(await screen.findByText("interview.wav")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("transcribe", expect.anything());

    fireEvent.click(screen.getByRole("button", { name: "Create transcript" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "transcribe",
        expect.objectContaining({ path: "/recordings/interview.wav" }),
      ),
    );
  });

  it("titles the draft with the recording name and shows it was saved locally", async () => {
    render(<App />);
    await importAndCreate();

    expect(screen.getByRole("heading", { name: "interview" })).toBeInTheDocument();
    expect(await screen.findByText("Saved locally")).toBeInTheDocument();
  });

  it("shows a failed local save in the header", async () => {
    saveSession = () => Promise.reject(new Error("disk full"));

    render(<App />);
    await importAndCreate();

    expect(await screen.findByText("Local save failed")).toBeInTheDocument();
  });

  it("saves a fresh correction before Import another clears it", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    invokeMock.mockClear();

    fireEvent.doubleClick(screen.getByTestId("paragraph"));
    const editor = screen.getByTestId("edit-sent-0");
    fireEvent.change(editor, { target: { value: "The corrected transcript." } });
    fireEvent.blur(editor);
    fireEvent.click(screen.getByRole("button", { name: "Import another" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "save_session",
        expect.objectContaining({
          session: expect.objectContaining({
            editorState: expect.objectContaining({
              sentences: [
                expect.objectContaining({
                  words: expect.arrayContaining([
                    expect.objectContaining({ text: "corrected" }),
                  ]),
                }),
              ],
            }),
          }),
        }),
      ),
    );
  });

  it("shows an edit as not yet saved until the save completes", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");

    fireEvent.doubleClick(screen.getByTestId("paragraph"));
    const editor = screen.getByTestId("edit-sent-0");
    fireEvent.change(editor, { target: { value: "The corrected transcript." } });
    fireEvent.blur(editor);

    expect(screen.getByTestId("save-status")).toHaveTextContent("Saving locally…");
  });

  it("keeps the running transcription when Import recording is clicked again", async () => {
    transcribe = () => new Promise(() => {});

    render(<App />);
    await screen.findByText("Import your first recording");
    fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
    fireEvent.click(await screen.findByRole("button", { name: "Choose recording" }));
    fireEvent.click(await screen.findByRole("button", { name: "Create transcript" }));
    await screen.findByTestId("transcribe-progress");

    fireEvent.click(screen.getByRole("button", { name: /Import recording/ }));

    expect(screen.getByTestId("transcribe-progress")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Choose recording" })).not.toBeInTheDocument();
  });

  it("rewrites the on-screen text when an undo lands during a slow save", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("A corrected draft.");
    await waitFor(() => expect(finishSave).toHaveLength(1), { timeout: 2000 });
    fireEvent.click(screen.getByTestId("undo"));
    saveSession = () => Promise.resolve();
    finishSave[0]!();

    await waitFor(
      () => expect(savedTexts().at(-1)).toBe("The local transcript is ready."),
      { timeout: 2000 },
    );
  });

  it("saves a correction made while Import another waits on a slow save", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("First correction.");
    fireEvent.click(screen.getByRole("button", { name: "Import another" }));
    await waitFor(() => expect(finishSave).toHaveLength(1));
    correctTo("Second correction.");
    saveSession = () => Promise.resolve();
    finishSave[0]!();

    await waitFor(() => expect(savedTexts()).toContain("Second correction."));
  });

  it("never writes another transcript's text under the one being left", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const firstId = invokeMock.mock.calls.find(([c]) => c === "save_session")?.[1]?.session?.id;
    sessions = [{ ...SECOND_SESSION, audioAvailable: true }];
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("First correction.");
    fireEvent.click(screen.getByRole("button", { name: "Import another" }));
    await waitFor(() => expect(finishSave).toHaveLength(1));
    fireEvent.click(screen.getByTestId("undo"));
    fireEvent.click(screen.getByRole("button", { name: /Transcripts/ }));
    fireEvent.click(await screen.findByTestId("open-second"));
    // Opening waits for the first transcript's write to finish.
    saveSession = () => Promise.resolve();
    finishSave[0]!();
    await waitFor(() =>
      expect(screen.getByTestId("paragraph")).toHaveTextContent("Second recording."),
    );
    correctTo("Edited second.");

    await waitFor(() => expect(savedTexts()).toContain("Edited second."), { timeout: 2000 });
    expect(screen.getByTestId("paragraph")).toHaveTextContent("Edited second.");
    const wrong = invokeMock.mock.calls.filter(
      ([c, args]) =>
        c === "save_session" &&
        (args as { session: { id: string } }).session.id === firstId &&
        JSON.stringify(args).includes("second"),
    );
    expect(wrong).toEqual([]);
  });

  it("keeps an undo made while an autosave is still writing, when leaving", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("A corrected draft.");
    await waitFor(() => expect(finishSave).toHaveLength(1), { timeout: 2000 });
    fireEvent.click(screen.getByTestId("undo"));
    fireEvent.click(screen.getByRole("button", { name: "Import another" }));
    saveSession = () => Promise.resolve();
    finishSave[0]!();

    await screen.findByRole("button", { name: "Choose recording" });
    expect(savedTexts().at(-1)).toBe("The local transcript is ready.");
  });

  it("does not write a transcript back after deleting it", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("First correction.");
    await waitFor(() => expect(finishSave).toHaveLength(1), { timeout: 2000 });
    correctTo("Second correction.");
    await new Promise((resolve) => setTimeout(resolve, 900));
    fireEvent.click(screen.getByTestId("delete-transcript"));
    saveSession = () => Promise.resolve();
    finishSave[0]!();

    await screen.findByText("Import your first recording");
    const commands = invokeMock.mock.calls.map(([command]) => command);
    expect(commands.lastIndexOf("save_session")).toBeLessThan(commands.indexOf("delete_matter"));
  });

  it("shows an undo during a running save as not yet saved", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("A corrected draft.");
    await waitFor(() => expect(finishSave).toHaveLength(1), { timeout: 2000 });
    fireEvent.click(screen.getByTestId("undo"));

    expect(screen.getByTestId("save-status")).toHaveTextContent("Saving locally…");
  });

  it("waits for saves queued during Import another before leaving", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    const finishSave: (() => void)[] = [];
    saveSession = () => new Promise((resolve) => finishSave.push(() => resolve(undefined)));

    correctTo("First correction.");
    await waitFor(() => expect(finishSave).toHaveLength(1), { timeout: 2000 });
    fireEvent.click(screen.getByRole("button", { name: "Import another" }));
    correctTo("Second correction.");
    await new Promise((resolve) => setTimeout(resolve, 900));
    fireEvent.click(screen.getByTestId("undo"));
    saveSession = () => Promise.resolve();
    finishSave[0]!();

    await screen.findByRole("button", { name: "Choose recording" });
    expect(savedTexts().at(-1)).toBe("First correction.");
  });
});
