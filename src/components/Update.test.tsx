import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Release } from "../lib/api";
import { MomentProvider } from "./ActionMoment";
import { ToastProvider } from "./ui";
import { useUpdate } from "./Update";

const mocks = vi.hoisted(() => ({
  updateStatus: vi.fn(),
  getSettings: vi.fn(),
  installUpdate: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api: mocks,
}));

const release: Release = { version: "0.2.0", notes: [] };

function Install({ ready = true }: { ready?: boolean }) {
  const update = useUpdate(ready);
  return (
    <button type="button" onClick={update.install}>
      install
    </button>
  );
}

function setup({ last, onboarded = true, available = null, ready = true }: { last: string | null; onboarded?: boolean; available?: Release | null; ready?: boolean }) {
  if (last) localStorage.setItem("last-version", last);
  mocks.updateStatus.mockResolvedValue({ current: "0.1.5", available });
  mocks.getSettings.mockResolvedValue({ onboarded });
  const tree = (ready: boolean) => (
    <ToastProvider>
      <MomentProvider>
        <Install ready={ready} />
      </MomentProvider>
    </ToastProvider>
  );
  const { rerender } = render(tree(ready));
  return { unlock: () => rerender(tree(true)) };
}

const settled = () => waitFor(() => expect(localStorage.getItem("last-version")).toBe("0.1.5"));

describe("update moment", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  it("appears after the version changed and remembers the new one", async () => {
    setup({ last: "0.1.4" });
    expect(await screen.findByText("Midnight 0.1.5")).toBeTruthy();
    expect(localStorage.getItem("last-version")).toBe("0.1.5");
  });

  it("waits until the app is unlocked", async () => {
    const { unlock } = setup({ last: "0.1.4", ready: false });
    await waitFor(() => expect(mocks.updateStatus).toHaveBeenCalled());
    expect(screen.queryByText("Midnight 0.1.5")).toBeNull();
    expect(localStorage.getItem("last-version")).toBe("0.1.4");
    unlock();
    expect(await screen.findByText("Midnight 0.1.5")).toBeTruthy();
  });

  it("stays away when the version is the same", async () => {
    setup({ last: "0.1.5" });
    await settled();
    await act(async () => {});
    expect(document.querySelector(".moment")).toBeNull();
  });

  it("stays away when the version went back, like after a fake update in development", async () => {
    setup({ last: "0.2.0" });
    await settled();
    await act(async () => {});
    expect(document.querySelector(".moment")).toBeNull();
  });

  it("compares versions by number, not by text", async () => {
    setup({ last: "0.1.10" });
    await settled();
    await act(async () => {});
    expect(document.querySelector(".moment")).toBeNull();
  });

  it("appears for the first version that remembers it, if the app was in use before", async () => {
    setup({ last: null });
    expect(await screen.findByText("Midnight 0.1.5")).toBeTruthy();
  });

  it("stays away on a fresh install", async () => {
    setup({ last: null, onboarded: false });
    await settled();
    await waitFor(() => expect(mocks.getSettings).toHaveBeenCalled());
    await act(async () => {});
    expect(document.querySelector(".moment")).toBeNull();
  });

  it("covers the screen while installing", async () => {
    mocks.installUpdate.mockReturnValue(new Promise(() => {}));
    setup({ last: "0.1.5", available: release });
    await settled();
    fireEvent.click(screen.getByText("install"));
    expect(await screen.findByText("Installing update")).toBeTruthy();
    expect(screen.getByText("Version 0.2.0, Midnight restarts afterwards")).toBeTruthy();
  });

  it("closes and reports the error when installing fails", async () => {
    mocks.installUpdate.mockRejectedValue(new Error("offline"));
    setup({ last: "0.1.5", available: release });
    await settled();
    fireEvent.click(screen.getByText("install"));
    await waitFor(() => expect(screen.getByText(/offline/)).toBeTruthy());
    expect(document.querySelector(".moment")).toBeNull();
  });
});
