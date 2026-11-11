import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import Sidebar from "./Sidebar";

// Sidebar is a pure presentational nav (no Tauri), so it renders straight in
// jsdom. These tests pin the shell's navigation contract: the nav items, the
// active-state signalling (aria-current), the onNavigate callback, and the
// always-visible privacy signal.

describe("Sidebar", () => {
  it("renders the nav items and the privacy chip", () => {
    render(<Sidebar view="library" onNavigate={() => {}} />);
    expect(screen.getByRole("button", { name: /Transcripts/ })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Import recording/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Models & settings/ })).toBeInTheDocument();
    expect(screen.getByText(/Local only/)).toBeInTheDocument();
  });

  it("marks only the active view with aria-current", () => {
    render(<Sidebar view="models" onNavigate={() => {}} />);
    expect(screen.getByRole("button", { name: /Models & settings/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: /Transcripts/ })).not.toHaveAttribute("aria-current");
  });

  it("calls onNavigate with the clicked view id", () => {
    const onNavigate = vi.fn();
    render(<Sidebar view="library" onNavigate={onNavigate} />);
    fireEvent.click(screen.getByRole("button", { name: /Import recording/ }));
    expect(onNavigate).toHaveBeenCalledWith("transcribe");
    fireEvent.click(screen.getByRole("button", { name: /Models & settings/ }));
    expect(onNavigate).toHaveBeenCalledWith("models");
  });
});
