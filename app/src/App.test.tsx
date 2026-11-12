import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Transcript } from "@core/types";
import { loadTranscript } from "@core/editor";

const invokeMock = vi.fn();
const openMock = vi.fn();
const listenMock = vi.fn();
let deleteTranscriptReport: { deleted: string[]; failed: unknown[] };
let pickRecording: () => Promise<unknown>;
let saveSession: () => Promise<unknown>;
let transcribe: () => Promise<unknown>;
let sessions: unknown[];
let startRecording: () => Promise<unknown>;
let discardRecording: () => Promise<unknown>;
let stopRecording: () => Promise<unknown>;
let interrupted: unknown[];
let recoverRecording: () => Promise<unknown>;
let deleteInterrupted: () => Promise<unknown>;
let modelPacks: unknown[];
// Event handlers registered through `listen`, so tests can emit backend events.
let handlers: Record<string, (event: { payload: unknown }) => void>;

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
  handlers = {};
  listenMock.mockImplementation((name: string, handler: (event: { payload: unknown }) => void) => {
    handlers[name] = handler;
    return Promise.resolve(() => {
      if (handlers[name] === handler) delete handlers[name];
    });
  });
  discardRecording = () => Promise.resolve();
  stopRecording = () => Promise.resolve({ transcript, speakersLabelled: false, note: null });
  startRecording = () =>
    Promise.resolve({ audioPath: "/library/recordings/Recording 2026-11-10 14-03-22.wav" });
  deleteTranscriptReport = { deleted: ["/recordings/interview.wav"], failed: [] };
  pickRecording = () => Promise.resolve("/recordings/interview.wav");
  saveSession = () => Promise.resolve();
  transcribe = () => Promise.resolve(transcript);
  sessions = [];
  interrupted = [];
  recoverRecording = () => Promise.resolve({ notes: "Client agreed to mediation." });
  deleteInterrupted = () => Promise.resolve();
  modelPacks = [
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
  ];
  invokeMock.mockImplementation((command: string) => {
    switch (command) {
      case "volume_encryption_status":
        return Promise.resolve({ state: "encrypted", detail: "test volume" });
      case "list_model_packs":
        return Promise.resolve(modelPacks);
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
      case "delete_transcript":
        return Promise.resolve(deleteTranscriptReport);
      case "delete_session":
        return Promise.resolve();
      case "start_recording":
        return startRecording();
      case "pause_recording":
      case "resume_recording":
        return Promise.resolve();
      case "discard_recording":
        return discardRecording();
      case "stop_recording":
        return stopRecording();
      case "save_recording_notes":
      case "forget_recording_notes":
        return Promise.resolve();
      case "list_interrupted_recordings":
        return Promise.resolve(interrupted);
      case "recover_recording":
        return recoverRecording();
      case "delete_interrupted_recording":
        return deleteInterrupted();
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

function savedNotes(): (string | undefined)[] {
  return invokeMock.mock.calls
    .filter(([command]) => command === "save_session")
    .map(([, args]) => (args as { session: { notes?: string } }).session.notes);
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
      numSpeakers: undefined,
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
      expect(invokeMock).toHaveBeenCalledWith("delete_transcript", { id: sessionId }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "delete_transcript",
      expect.objectContaining({ paths: expect.anything() }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("delete_session", expect.anything());
  });

  it("keeps the transcript when source-audio deletion reports a failure", async () => {
    deleteTranscriptReport = {
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
    expect(commands.lastIndexOf("save_session")).toBeLessThan(commands.indexOf("delete_transcript"));
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

describe("speaker count", () => {
  // Tauri commands take camelCase argument names by default; a snake_case key
  // is silently dropped and the backend falls back to automatic detection.
  it("reaches the transcribe command under Tauri's argument name", async () => {
    render(<App />);
    await screen.findByText("Import your first recording");
    fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
    fireEvent.click(await screen.findByRole("button", { name: "Choose recording" }));
    fireEvent.click(await screen.findByTestId("detect-speakers"));
    fireEvent.change(screen.getByTestId("speaker-count"), { target: { value: "2" } });
    fireEvent.click(await screen.findByRole("button", { name: "Create transcript" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("transcribe", expect.anything()),
    );
    expect(invokeMock).toHaveBeenCalledWith(
      "transcribe",
      expect.objectContaining({ numSpeakers: 2 }),
    );
  });
});

describe("Record", () => {
  async function startRecordingFromSidebar(): Promise<void> {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /^Record/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Start recording" }));
    await screen.findByText("Recording");
  }

  function emit(name: string, payload: unknown): void {
    act(() => handlers[name]!({ payload }));
  }

  it("shows each utterance as it is transcribed, in order", async () => {
    await startRecordingFromSidebar();
    expect(invokeMock).toHaveBeenCalledWith("start_recording", { model: "small.en" });

    emit("record-utterance", { start: 0.4, end: 3.1, text: "Thanks for coming in.", words: [] });
    emit("record-utterance", { start: 4.0, end: 6.2, text: "Happy to help.", words: [] });

    const live = screen.getByTestId("live-transcript");
    expect(live).toHaveTextContent(/Thanks for coming in\..*Happy to help\./);
  });

  it("pauses and resumes the recording", async () => {
    await startRecordingFromSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(await screen.findByRole("button", { name: "Resume" })).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("pause_recording");

    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    expect(await screen.findByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("resume_recording");
  });

  it("stops with the chosen speaker count and opens the saved transcript", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /^Record/ }));
    fireEvent.click(await screen.findByTestId("label-speakers"));
    fireEvent.click(screen.getByRole("button", { name: "More speakers" }));
    fireEvent.click(await screen.findByRole("button", { name: "Start recording" }));
    await screen.findByText("Recording");

    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));

    expect(
      await screen.findByRole("heading", { name: "Recording 2026-11-10 14-03-22" }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("stop_recording", { diarize: true, numSpeakers: 2 });
    await waitFor(() => expect(savedTexts()).toHaveLength(1));
    const saved = invokeMock.mock.calls.find(([command]) => command === "save_session")!;
    expect((saved[1] as { session: { audioPath: string } }).session.audioPath).toBe(
      "/library/recordings/Recording 2026-11-10 14-03-22.wav",
    );
  });

  it("keeps the recording page while recording", async () => {
    await startRecordingFromSidebar();
    const listings = () => invokeMock.mock.calls.filter(([c]) => c === "list_sessions").length;
    const before = listings();

    fireEvent.click(screen.getByRole("button", { name: /Transcripts/ }));

    expect(
      await screen.findByText("Stop or discard the recording before leaving this page."),
    ).toBeInTheDocument();
    expect(screen.getByText("Recording")).toBeInTheDocument();
    expect(listings()).toBe(before);
  });

  it("discards the recording only after confirmation", async () => {
    await startRecordingFromSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(invokeMock).not.toHaveBeenCalledWith("discard_recording");
    fireEvent.click(screen.getByRole("button", { name: "Delete recording" }));

    expect(await screen.findByRole("button", { name: "Start recording" })).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("discard_recording");
  });

  it("shows why recording could not start", async () => {
    startRecording = () => Promise.reject("No microphone found. Connect one and try again.");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /^Record/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Start recording" }));

    expect(
      await screen.findByText("No microphone found. Connect one and try again."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start recording" })).toBeInTheDocument();
    expect(screen.queryByText("Recording")).not.toBeInTheDocument();
  });

  it("keeps a transcript whose correction could not be saved instead of opening Record", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");
    saveSession = () => Promise.reject(new Error("disk full"));
    correctTo("Unsaved correction.");

    fireEvent.click(screen.getByRole("button", { name: /^Record/ }));

    expect(await screen.findByText("Local save failed")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "interview" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start recording" })).not.toBeInTheDocument();
  });

  it("does not start recording while an import is still transcribing", async () => {
    transcribe = () => new Promise(() => {});
    render(<App />);
    await screen.findByText("Import your first recording");
    fireEvent.click(screen.getAllByRole("button", { name: /Import recording/ })[0]!);
    fireEvent.click(await screen.findByRole("button", { name: "Choose recording" }));
    fireEvent.click(await screen.findByRole("button", { name: "Create transcript" }));
    await screen.findByTestId("transcribe-progress");

    fireEvent.click(screen.getByRole("button", { name: /^Record/ }));

    expect(await screen.findByRole("button", { name: "Start recording" })).toBeDisabled();
  });

  it("asks the backend to discard a recording only once", async () => {
    discardRecording = () => new Promise(() => {});
    await startRecordingFromSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete recording" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete recording" }));

    expect(
      invokeMock.mock.calls.filter(([command]) => command === "discard_recording"),
    ).toHaveLength(1);
  });

  it("warns while the microphone delivers no sound, and clears when it does", async () => {
    await startRecordingFromSidebar();

    emit("record-warnings", { silent: true, narrowband: false });
    expect(screen.getByText(/receiving no sound from the microphone/)).toBeInTheDocument();

    emit("record-warnings", { silent: false, narrowband: false });
    expect(screen.queryByText(/receiving no sound from the microphone/)).not.toBeInTheDocument();
  });

  it("shows that the microphone is reconnecting", async () => {
    await startRecordingFromSidebar();

    emit("record-status", { message: "Microphone disconnected — reconnecting…", transient: false });

    expect(screen.getByText("Microphone disconnected — reconnecting…")).toBeInTheDocument();
  });

  it("says when live text is falling behind", async () => {
    await startRecordingFromSidebar();

    emit("record-level", { level: 0.2, speaking: false, elapsedS: 60, backlogS: 40 });
    expect(screen.getByText(/Live text is catching up \(about 40 s behind\)/)).toBeInTheDocument();

    emit("record-level", { level: 0.2, speaking: false, elapsedS: 61, backlogS: 2 });
    expect(screen.queryByText(/Live text is catching up/)).not.toBeInTheDocument();
  });

  it("shows which microphone is recording, and follows a switch", async () => {
    await startRecordingFromSidebar();

    emit("record-device", "MacBook Pro Microphone");
    expect(screen.getByTestId("recording-device")).toHaveTextContent("MacBook Pro Microphone");

    emit("record-device", "WH-1000XM5");
    expect(screen.getByTestId("recording-device")).toHaveTextContent("WH-1000XM5");
  });

  it("labels speakers when turned on during a recording, with the number set", async () => {
    await startRecordingFromSidebar();
    expect(screen.getByTestId("speaker-summary")).toHaveTextContent("Speakers won't be labelled.");

    fireEvent.click(screen.getByTestId("label-speakers"));
    fireEvent.click(screen.getByRole("button", { name: "More speakers" }));
    fireEvent.click(screen.getByRole("button", { name: "More speakers" }));
    expect(screen.getByTestId("speaker-summary")).toHaveTextContent(
      "Speakers will be labelled when you stop, assuming 3 people.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));

    await screen.findByRole("heading", { name: "Recording 2026-11-10 14-03-22" });
    expect(invokeMock).toHaveBeenCalledWith("stop_recording", { diarize: true, numSpeakers: 3 });
  });

  it("keeps notes written during a recording with the transcript", async () => {
    await startRecordingFromSidebar();

    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Your notes" }), {
      target: { value: "Client agreed to mediation." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));

    await screen.findByRole("heading", { name: "Recording 2026-11-10 14-03-22" });
    await waitFor(() => expect(savedNotes()).toEqual(["Client agreed to mediation."]));
    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    expect(screen.getByRole("textbox", { name: "Your notes" })).toHaveValue(
      "Client agreed to mediation.",
    );
  });

  it("keeps notes typed while the recording is finishing", async () => {
    let finishStop: (value: unknown) => void = () => {};
    stopRecording = () => new Promise((resolve) => (finishStop = resolve));
    await startRecordingFromSidebar();
    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    const box = screen.getByRole("textbox", { name: "Your notes" });
    fireEvent.change(box, { target: { value: "First point." } });

    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("stop_recording", expect.anything()),
    );
    fireEvent.change(box, { target: { value: "First point. Second point." } });
    await act(async () => finishStop({ transcript, speakersLabelled: false, note: null }));

    await waitFor(() => expect(savedNotes()).toEqual(["First point. Second point."]));
  });

  it("backs up notes at Stop and while the recording is finishing", async () => {
    stopRecording = () => new Promise(() => {});
    await startRecordingFromSidebar();
    const audioPath = "/library/recordings/Recording 2026-11-10 14-03-22.wav";
    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    const box = screen.getByRole("textbox", { name: "Your notes" });
    fireEvent.change(box, { target: { value: "First point." } });

    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_recording_notes", {
        audioPath,
        text: "First point.",
      }),
    );

    fireEvent.change(box, { target: { value: "First point. Second point." } });
    await waitFor(
      () =>
        expect(invokeMock).toHaveBeenCalledWith("save_recording_notes", {
          audioPath,
          text: "First point. Second point.",
        }),
      { timeout: 3000 },
    );
  });

  it("keeps the recording's notes file until the transcript holding the notes is saved", async () => {
    saveSession = () => Promise.reject(new Error("disk full"));
    await startRecordingFromSidebar();
    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Your notes" }), {
      target: { value: "Client agreed to mediation." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));

    expect(await screen.findByText("Local save failed")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("forget_recording_notes", expect.anything());

    saveSession = () => Promise.resolve();
    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Your notes" }), {
      target: { value: "Client agreed to mediation. Costs to follow." },
    });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("forget_recording_notes", {
        audioPath: "/library/recordings/Recording 2026-11-10 14-03-22.wav",
      }),
    );
  });

  it("opens the transcript without speakers, and says why, when labelling fails", async () => {
    stopRecording = () =>
      Promise.resolve({
        transcript,
        speakersLabelled: false,
        note: "Speakers couldn't be labelled, so this transcript has none. (no speaker model)",
      });
    await startRecordingFromSidebar();
    fireEvent.click(screen.getByTestId("label-speakers"));

    fireEvent.click(screen.getByRole("button", { name: "Stop and create transcript" }));

    expect(
      await screen.findByText(
        "Speakers couldn't be labelled, so this transcript has none. (no speaker model)",
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId("paragraph")).toBeInTheDocument();
    await waitFor(() => expect(savedNotes()).toHaveLength(1));
    const saved = invokeMock.mock.calls.find(([command]) => command === "save_session")!;
    expect((saved[1] as { session: { diarized: boolean } }).session.diarized).toBe(false);
  });
});

describe("Your notes", () => {
  it("saves notes edited on an open transcript", async () => {
    render(<App />);
    await importAndCreate();
    await screen.findByText("Saved locally");

    fireEvent.click(screen.getByRole("tab", { name: "Your notes" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Your notes" }), {
      target: { value: "Follow up on the tenancy dates." },
    });

    await waitFor(() => expect(savedNotes()).toContain("Follow up on the tenancy dates."));
  });
});

describe("Recordings without a transcript", () => {
  const LEFT = {
    audioPath: "/library/recordings/Recording 2026-11-12 14-03-22.wav",
    name: "Recording 2026-11-12 14-03-22",
    durationSec: 2520,
    hasNotes: true,
  };

  it("offers at launch to recover one, with its notes, into a saved transcript", async () => {
    interrupted = [LEFT];
    render(<App />);

    const notice = await screen.findByTestId("interrupted-recording");
    expect(notice).toHaveTextContent("Recording 2026-11-12 14-03-22 (42 min, with notes)");
    fireEvent.click(screen.getByRole("button", { name: "Recover transcript" }));

    expect(
      await screen.findByText(
        "Recovered from a recording that had no transcript. The last moment of audio may be missing.",
      ),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("transcribe", expect.objectContaining({ path: LEFT.audioPath }));
    await waitFor(() => expect(savedNotes()).toEqual(["Client agreed to mediation."]));
    const saved = invokeMock.mock.calls.find(([command]) => command === "save_session")!;
    expect((saved[1] as { session: { audioPath: string } }).session.audioPath).toBe(LEFT.audioPath);
    expect(screen.queryByTestId("interrupted-recording")).not.toBeInTheDocument();
  });

  it("starts only one recovery at a time", async () => {
    let prepared!: (value: unknown) => void;
    recoverRecording = () => new Promise((resolve) => (prepared = resolve));
    interrupted = [LEFT, { ...LEFT, audioPath: "/library/recordings/Recording 2.wav", name: "Recording 2" }];
    render(<App />);

    fireEvent.click((await screen.findAllByRole("button", { name: "Recover transcript" }))[0]!);
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([c]) => c === "recover_recording")).toHaveLength(1),
    );
    fireEvent.click(screen.getAllByRole("button", { name: "Recover transcript" })[1]!);
    prepared({ notes: "" });

    await screen.findByTestId("paragraph");
    expect(invokeMock.mock.calls.filter(([c]) => c === "recover_recording")).toHaveLength(1);
  });

  it("says, in the notice, why a recording could not be recovered or deleted", async () => {
    modelPacks = [];
    deleteInterrupted = () => Promise.reject(new Error("permission denied"));
    interrupted = [LEFT];
    render(<App />);
    const notice = await screen.findByTestId("interrupted-recording");

    fireEvent.click(screen.getByRole("button", { name: "Recover transcript" }));
    expect(
      await within(notice).findByText(/install a verified transcription model/),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete permanently" }));
    expect(await within(notice).findByText(/permission denied/)).toBeInTheDocument();
  });

  it("deletes one only after confirmation", async () => {
    interrupted = [LEFT];
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    expect(invokeMock).not.toHaveBeenCalledWith("delete_interrupted_recording", expect.anything());
    fireEvent.click(screen.getByRole("button", { name: "Delete permanently" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_interrupted_recording", {
        audioPath: LEFT.audioPath,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("interrupted-recording")).not.toBeInTheDocument(),
    );
  });
});
