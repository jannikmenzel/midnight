import { act, fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MomentProvider, useMoment, type Moment } from "./ActionMoment";

type Api = ReturnType<typeof useMoment>;
const done: Moment = { kind: "export", title: "Done", detail: "12 items" };
const working: Moment = { kind: "export", title: "Working" };

function setup() {
  let api: Api | undefined;
  const Capture = () => {
    const moment = useMoment();
    useEffect(() => {
      api = moment;
    }, [moment]);
    return null;
  };
  render(
    <MomentProvider>
      <Capture />
    </MomentProvider>,
  );
  return api as Api;
}

const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));
const visible = () => document.querySelector(".moment") !== null;

describe("full-screen moments", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it("shows a finished action, holds it and resolves once it is gone", async () => {
    const moment = setup();
    const closed = vi.fn();
    act(() => void moment.show(done).then(closed));
    expect(screen.getByText("Done")).toBeTruthy();
    await advance(2500);
    expect(visible()).toBe(true);
    await advance(100);
    await advance(280);
    expect(visible()).toBe(false);
    expect(closed).toHaveBeenCalledOnce();
  });

  it("stays in the working state for as long as the task runs", async () => {
    const moment = setup();
    let finish = (_: number) => {};
    const task = new Promise<number>((resolve) => (finish = resolve));
    const result = vi.fn();
    act(() => void moment.run(working, () => task, () => done).then(result));
    await advance(10_000);
    expect(screen.getByText("Working")).toBeTruthy();
    await act(async () => finish(7));
    expect(screen.getByText("Done")).toBeTruthy();
    expect(result).not.toHaveBeenCalled();
    await advance(2000);
    await advance(280);
    expect(result).toHaveBeenCalledWith(7);
    expect(visible()).toBe(false);
  });

  it("closes at once and passes on the error when the task fails", async () => {
    const moment = setup();
    const failed = vi.fn();
    act(() => void moment.run(working, () => Promise.reject(new Error("boom")), () => done).catch(failed));
    await advance(0);
    expect(visible()).toBe(false);
    expect(failed.mock.calls[0][0].message).toBe("boom");
    expect(screen.queryByText("Done")).toBeNull();
  });

  it("cannot be skipped while working, but after that by click or Esc", async () => {
    const moment = setup();
    act(() => void moment.run(working, () => Promise.resolve(1), () => done));
    const overlay = () => document.querySelector(".moment") as HTMLElement;
    fireEvent.click(overlay());
    fireEvent.keyDown(document.body, { key: "Escape" });
    await advance(500);
    expect(screen.getByText("Working")).toBeTruthy();
    await advance(1000);
    fireEvent.keyDown(document.body, { key: "Escape" });
    await advance(280);
    expect(visible()).toBe(false);
  });

  it("keeps the keyboard away from the app behind it", () => {
    const moment = setup();
    const behind = vi.fn();
    window.addEventListener("keydown", behind);
    act(() => void moment.show(done));
    for (const key of ["Enter", "Tab", " ", "k"]) {
      expect(fireEvent.keyDown(document.body, { key })).toBe(false);
    }
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(fireEvent.keyDown(document.body, { key: "k", metaKey: true })).toBe(true);
    expect(behind).not.toHaveBeenCalled();
    window.removeEventListener("keydown", behind);
  });

  it("keeps the keyboard working again afterwards", async () => {
    const moment = setup();
    const behind = vi.fn();
    window.addEventListener("keydown", behind);
    act(() => void moment.show(done));
    await advance(2600);
    await advance(280);
    fireEvent.keyDown(document.body, { key: "Enter" });
    expect(behind).toHaveBeenCalledOnce();
    window.removeEventListener("keydown", behind);
  });
});
