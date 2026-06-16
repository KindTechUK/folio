import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import Sidebar from "./Sidebar";

// Sidebar is a pure presentational nav (no Tauri), so it renders straight in
// jsdom. These tests pin the shell's navigation contract: the nav items, the
// active-state signalling (aria-pressed), the onNavigate callback, and the
// always-visible privacy signal.

describe("Sidebar", () => {
  it("renders the wordmark, the three nav items, and the privacy chip", () => {
    render(<Sidebar view="library" onNavigate={() => {}} />);
    expect(screen.getByText("Kindtech")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Library/ })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /New transcription/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Models/ })).toBeInTheDocument();
    expect(screen.getByText(/Local only/)).toBeInTheDocument();
  });

  it("marks only the active view with aria-pressed", () => {
    render(<Sidebar view="models" onNavigate={() => {}} />);
    expect(screen.getByRole("button", { name: /Models/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /Library/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("calls onNavigate with the clicked view id", () => {
    const onNavigate = vi.fn();
    render(<Sidebar view="library" onNavigate={onNavigate} />);
    fireEvent.click(screen.getByRole("button", { name: /New transcription/ }));
    expect(onNavigate).toHaveBeenCalledWith("transcribe");
    fireEvent.click(screen.getByRole("button", { name: /Models/ }));
    expect(onNavigate).toHaveBeenCalledWith("models");
  });
});
