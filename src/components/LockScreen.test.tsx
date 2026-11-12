import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LockScreen } from "./LockScreen";

const mock = vi.hoisted(() => ({
  calls: [] as string[],
  answer: (_pin: string): Promise<boolean> => Promise.resolve(true),
  cooldown: null as number | null,
  touch: false,
  finger: (): Promise<boolean> => Promise.resolve(true),
  prompts: 0,
}));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api: {
    unlock: (pin: string) => {
      mock.calls.push(pin);
      return mock.answer(pin);
    },
    pinCooldown: () => Promise.resolve(mock.cooldown),
    touchIdStatus: () => Promise.resolve({ available: true, enabled: mock.touch }),
    unlockTouchId: () => {
      mock.prompts++;
      return mock.finger();
    },
  },
}));

const type = (value: string) => fireEvent.change(screen.getByLabelText("PIN"), { target: { value } });

describe("LockScreen", () => {
  beforeEach(() => {
    mock.calls = [];
    mock.cooldown = null;
    mock.touch = false;
    mock.prompts = 0;
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
  });

  it("checks the PIN once five digits are in and opens on success", async () => {
    mock.answer = () => Promise.resolve(true);
    const onUnlock = vi.fn();
    render(<LockScreen onUnlock={onUnlock} />);
    type("12a3");
    expect(mock.calls).toEqual([]);
    expect((screen.getByLabelText("PIN") as HTMLInputElement).value).toBe("123");
    type("12345");
    await waitFor(() => expect(onUnlock).toHaveBeenCalled());
    expect(mock.calls).toEqual(["12345"]);
  });

  it("clears the input and marks it when the PIN is wrong", async () => {
    mock.answer = () => Promise.resolve(false);
    const onUnlock = vi.fn();
    render(<LockScreen onUnlock={onUnlock} />);
    type("99999");
    await waitFor(() => expect(screen.getByLabelText("PIN").getAttribute("aria-invalid")).toBe("true"));
    expect((screen.getByLabelText("PIN") as HTMLInputElement).value).toBe("");
    expect(onUnlock).not.toHaveBeenCalled();
    type("1");
    expect(screen.getByLabelText("PIN").getAttribute("aria-invalid")).toBeNull();
  });

  it("blocks the input while the pause runs", async () => {
    mock.answer = () => Promise.reject("Too many attempts. Try again in 30 s.");
    mock.cooldown = 30;
    render(<LockScreen onUnlock={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("PIN") as HTMLInputElement).disabled).toBe(true));
  });

  it("asks for Touch ID right away and opens on success", async () => {
    mock.touch = true;
    mock.finger = () => Promise.resolve(true);
    const onUnlock = vi.fn();
    render(<LockScreen onUnlock={onUnlock} />);
    await waitFor(() => expect(onUnlock).toHaveBeenCalled());
    expect(mock.prompts).toBe(1);
  });

  it("keeps the PIN and a retry button when Touch ID is cancelled", async () => {
    mock.touch = true;
    mock.finger = () => Promise.resolve(false);
    const onUnlock = vi.fn();
    render(<LockScreen onUnlock={onUnlock} />);
    const retry = await screen.findByRole("button", { name: "Touch ID" });
    await waitFor(() => expect((retry as HTMLButtonElement).disabled).toBe(false));
    expect(mock.prompts).toBe(1);
    expect((screen.getByLabelText("PIN") as HTMLInputElement).disabled).toBe(false);
    mock.finger = () => Promise.resolve(true);
    fireEvent.click(retry);
    await waitFor(() => expect(onUnlock).toHaveBeenCalled());
    expect(mock.prompts).toBe(2);
  });

  it("waits until the window has focus before asking", async () => {
    mock.touch = true;
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const onUnlock = vi.fn();
    render(<LockScreen onUnlock={onUnlock} />);
    await screen.findByRole("button", { name: "Touch ID" });
    expect(mock.prompts).toBe(0);
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    fireEvent.focus(window);
    await waitFor(() => expect(onUnlock).toHaveBeenCalled());
    expect(mock.prompts).toBe(1);
  });

  it("shows no Touch ID button when it is off", async () => {
    render(<LockScreen onUnlock={() => {}} />);
    await waitFor(() => expect(screen.getByLabelText("PIN")).toBeTruthy());
    expect(screen.queryByRole("button", { name: "Touch ID" })).toBeNull();
    expect(mock.prompts).toBe(0);
  });
});
