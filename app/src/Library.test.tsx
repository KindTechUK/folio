import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import type { SessionMeta } from "./session";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import Library from "./Library";

const sessions: SessionMeta[] = [
  {
    id: "transcript-1",
    createdAt: "2026-07-08T09:00:00.000Z",
    updatedAt: "2026-07-08T10:00:00.000Z",
    audioFilename: "client-interview.m4a",
    diarized: true,
    segments: 42,
    audioAvailable: true,
  },
  {
    id: "transcript-2",
    createdAt: "2026-07-04T09:00:00.000Z",
    updatedAt: "2026-07-04T10:00:00.000Z",
    audioFilename: "witness-statement.wav",
    diarized: false,
    segments: 18,
    audioAvailable: false,
  },
];

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(sessions);
});

describe("Library", () => {
  it("lists transcripts with mode and audio availability", async () => {
    render(<Library onOpen={() => {}} onImport={() => {}} />);

    expect(await screen.findByText("client-interview.m4a")).toBeInTheDocument();
    expect(screen.getByText("witness-statement.wav")).toBeInTheDocument();
    expect(screen.getByText("Speakers")).toBeInTheDocument();
    expect(screen.getByText("Standard")).toBeInTheDocument();
    expect(screen.getByText("Missing")).toBeInTheDocument();
  });

  it("filters transcripts by filename", async () => {
    render(<Library onOpen={() => {}} onImport={() => {}} />);
    await screen.findByText("client-interview.m4a");

    fireEvent.change(screen.getByRole("searchbox", { name: "Search transcripts" }), {
      target: { value: "witness" },
    });

    expect(screen.queryByText("client-interview.m4a")).not.toBeInTheDocument();
    expect(screen.getByText("witness-statement.wav")).toBeInTheDocument();
  });

  it("starts the import flow from the primary action", async () => {
    const onImport = vi.fn();
    render(<Library onOpen={() => {}} onImport={onImport} />);
    await screen.findByText("client-interview.m4a");

    fireEvent.click(screen.getByRole("button", { name: "Import recording" }));
    expect(onImport).toHaveBeenCalledOnce();
  });
});
