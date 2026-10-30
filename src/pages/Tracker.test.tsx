import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../lib/api";
import { AppContext } from "../lib/nav";
import { MomentProvider } from "../components/ActionMoment";
import { ToastProvider } from "../components/ui";
import Tracker from "./Tracker";

const mocks = vi.hoisted(() => ({
  exportTimesheet: vi.fn(),
  openPath: vi.fn(() => Promise.resolve()),
  runningTimer: vi.fn(() => Promise.resolve(null)),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), save: () => Promise.resolve("/tmp/export.pdf") }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api: new Proxy(mocks, { get: (target, key: string) => target[key as keyof typeof mocks] ?? (() => Promise.resolve([])) }),
}));

const settings = { base_currency: "EUR", limits: {} } as unknown as Settings;
const context = { route: "tracker" as const, params: {}, go: () => {}, settings, reloadSettings: async () => {}, theme: "light" as const, setTheme: () => {} };

const title = "Export time tracking";
const dialogState = () => screen.queryByText(title)?.closest("[data-state]")?.getAttribute("data-state");

async function exportNow() {
  render(
    <ToastProvider>
      <MomentProvider>
        <AppContext.Provider value={context}>
          <Tracker />
        </AppContext.Provider>
      </MomentProvider>
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByText("Export"));
  fireEvent.click(await screen.findByText("Export as PDF"));
}

describe("time tracking export", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("closes the dialog as soon as the moment starts", async () => {
    mocks.exportTimesheet.mockReturnValue(new Promise(() => {}));
    await exportNow();
    expect(await screen.findByText("Preparing time tracking export")).toBeTruthy();
    await waitFor(() => expect(dialogState() ?? "closed").toBe("closed"));
  });

  it("opens the dialog again when the export fails", async () => {
    mocks.exportTimesheet.mockRejectedValue(new Error("no xelatex"));
    await exportNow();
    await waitFor(() => expect(screen.getByText(/no xelatex/)).toBeTruthy());
    expect(document.querySelector(".moment")).toBeNull();
    await waitFor(() => expect(dialogState()).toBe("open"));
  });
});
