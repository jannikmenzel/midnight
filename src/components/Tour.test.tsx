import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../lib/api";
import { AppContext } from "../lib/nav";
import { tourDone } from "../lib/tour";
import { Tour } from "./Tour";

function renderTour(go = vi.fn(), onClose = vi.fn()) {
  const context = { route: "overview" as const, params: {}, go, settings: {} as Settings, reloadSettings: async () => {}, theme: "light" as const, setTheme: () => {} };
  const ui = (open: boolean) => (
    <AppContext.Provider value={context}>
      <button type="button" data-tour="search">
        Search
      </button>
      <Tour open={open} onClose={onClose} />
    </AppContext.Provider>
  );
  const { rerender } = render(ui(true));
  const reopen = () => {
    rerender(ui(false));
    act(() => vi.advanceTimersByTime(500));
    rerender(ui(true));
  };
  return { go, onClose, reopen };
}

describe("Tour", () => {
  beforeEach(() => localStorage.clear());

  it("can be skipped and is not shown again", () => {
    expect(tourDone()).toBe(false);
    const { onClose } = renderTour();
    expect(screen.getByText("Welcome to Midnight")).toBeTruthy();
    fireEvent.click(screen.getByText("Skip"));
    expect(onClose).toHaveBeenCalled();
    expect(tourDone()).toBe(true);
  });

  it("walks through the steps and opens the pages", () => {
    const { go } = renderTour();
    fireEvent.click(screen.getByText("Start tour"));
    expect(screen.getByText("Search and actions")).toBeTruthy();
    fireEvent.click(screen.getByText("Continue"));
    fireEvent.click(screen.getByText("Continue"));
    expect(screen.getByText("Invoices")).toBeTruthy();
    expect(go).toHaveBeenCalledWith("invoices");
    fireEvent.keyDown(window, { key: "ArrowLeft" });
    expect(screen.getByText("Overview")).toBeTruthy();
    expect(tourDone()).toBe(false);
  });

  it("is skipped with Escape and starts at the beginning when opened again", () => {
    vi.useFakeTimers();
    const { onClose, reopen } = renderTour();
    fireEvent.click(screen.getByText("Start tour"));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    expect(tourDone()).toBe(true);
    reopen();
    expect(screen.getByText("Welcome to Midnight")).toBeTruthy();
  });

  it("is marked as done when finished", () => {
    const { onClose } = renderTour();
    while (!screen.queryByText("Done")) fireEvent.click(screen.getByText(/Start tour|Continue/));
    expect(screen.getByText("That's it")).toBeTruthy();
    fireEvent.click(screen.getByText("Done"));
    expect(onClose).toHaveBeenCalled();
    expect(tourDone()).toBe(true);
  });
});
