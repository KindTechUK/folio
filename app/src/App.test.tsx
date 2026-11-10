import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Transcript } from "@core/types";

const invokeMock = vi.fn();
const openMock = vi.fn();
const listenMock = vi.fn();
let deleteMatterReport: { deleted: string[]; failed: unknown[] };
let pickRecording: () => Promise<unknown>;
let saveSession: () => Promise<unknown>;

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
        return Promise.resolve([]);
      case "pick_recording":
        return pickRecording();
      case "transcribe":
        return Promise.resolve(transcript);
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
});
