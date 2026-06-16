import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import Banner from "./Banner";

// Banner is the single notice component for the app (privacy/warning/info). It
// forwards role / data-testid / aria-label to the root and renders a dismiss
// button only when onDismiss is given — the contract App.tsx relies on for the
// BitLocker warning and the machine-draft disclaimer.

describe("Banner", () => {
  it("forwards role, data-testid and aria-label to the root element", () => {
    render(
      <Banner
        variant="warning"
        role="alert"
        data-testid="enc-warning"
        aria-label="encryption warning"
      >
        volume not encrypted
      </Banner>,
    );
    const el = screen.getByTestId("enc-warning");
    expect(el).toHaveAttribute("role", "alert");
    expect(el).toHaveAttribute("aria-label", "encryption warning");
    expect(el).toHaveTextContent("volume not encrypted");
  });

  it("renders no dismiss button without onDismiss", () => {
    render(<Banner variant="info">draft notice</Banner>);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("renders a dismiss button that fires onDismiss", () => {
    const onDismiss = vi.fn();
    render(
      <Banner
        variant="warning"
        onDismiss={onDismiss}
        dismissLabel="dismiss warning"
        dismissTestid="dismiss-warning"
      >
        heads up
      </Banner>,
    );
    fireEvent.click(screen.getByTestId("dismiss-warning"));
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
