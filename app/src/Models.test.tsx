import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import type { PackStatus } from "./Models";

// Mock the Tauri IPC + dialog so the component can be exercised in jsdom. The
// backend's real verify/import logic is covered by the Rust `verify_pack_dir`
// tests against the shared fixtures; here we assert the UI renders the
// verified/rejected status the backend reports and gates import on it.
const invokeMock = vi.fn();
const openMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: (...a: unknown[]) => openMock(...a) }));

import Models, { selectAsrModel } from "./Models";

const verified: PackStatus = {
  dir: "whisper-small-en",
  id: "whisper-small-en",
  version: "1.2.0",
  engine: "sherpa-onnx",
  task: "asr",
  attribution: "OpenAI Whisper",
  verified: true,
  reason: null,
};
const rejected: PackStatus = {
  dir: "tampered-pack",
  id: "tampered-pack",
  version: "1.0.0",
  engine: "sherpa-onnx",
  task: "asr",
  attribution: "Someone",
  verified: false,
  reason: "weights hash mismatch (tampered or corrupt)",
};

beforeEach(() => {
  invokeMock.mockReset();
  openMock.mockReset();
});

describe("Models view", () => {
  it("selects the engine model provided by a verified ASR pack", () => {
    expect(selectAsrModel([verified])).toBe("small.en");
    expect(selectAsrModel([{ ...verified, id: "asr-parakeet-v2" }])).toBe("parakeet");
    expect(selectAsrModel([{ ...verified, verified: false }])).toBeNull();
    expect(selectAsrModel([{ ...verified, task: "diarization" }])).toBeNull();
  });

  it("lists packs with verified ✓ and rejected ✗ + reason", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_model_packs") return Promise.resolve([verified, rejected]);
      throw new Error(`unexpected command ${cmd}`);
    });

    render(<Models />);

    expect(await screen.findByText(/✓ verified/)).toBeInTheDocument();
    expect(screen.getByText(/✗ rejected/)).toBeInTheDocument();
    expect(screen.getByText("whisper-small-en")).toBeInTheDocument();
    // the rejection reason is surfaced
    expect(
      screen.getByText("weights hash mismatch (tampered or corrupt)"),
    ).toBeInTheDocument();
  });

  it("shows 'no packs' when the dir is empty", async () => {
    invokeMock.mockResolvedValue([]);
    render(<Models />);
    expect(await screen.findByText(/No model packs installed/)).toBeInTheDocument();
  });

  it("surfaces the rejection reason and does not list an unsigned import as verified", async () => {
    // initial list empty; the import is rejected (fail closed).
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_model_packs") return Promise.resolve([]);
      if (cmd === "import_model_pack")
        return Promise.resolve({
          ...rejected,
          dir: "unsigned-pack",
          id: "unsigned-pack",
          reason: "signature invalid (unsigned or untrusted pack)",
        } satisfies PackStatus);
      throw new Error(`unexpected command ${cmd}`);
    });
    openMock.mockResolvedValue("/some/picked/dir");

    render(<Models />);
    await screen.findByText(/No model packs installed/);

    fireEvent.click(screen.getByRole("button", { name: /Import model pack/ }));

    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toMatch(
        /rejected — signature invalid/,
      ),
    );
    // import_model_pack was called with the camelCase srcDir arg.
    expect(invokeMock).toHaveBeenCalledWith("import_model_pack", {
      srcDir: "/some/picked/dir",
    });
  });
});
