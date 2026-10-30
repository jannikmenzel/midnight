import { act, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setReducedMotion } from "../test-setup";
import { money } from "../lib/format";
import { AnimatedMoney } from "./ui";

const shown = (root: HTMLElement) => (root.querySelector("[data-count]") as HTMLElement).textContent;
const reserved = (root: HTMLElement) => (root.querySelector("[data-final]") as HTMLElement).textContent;
const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

describe("AnimatedMoney", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it("counts up from 0 and ends on the exact amount", async () => {
    const { container } = render(<AnimatedMoney cents={5835463} />);
    expect(shown(container)).toBe(money(0));
    await advance(100);
    expect(shown(container)).not.toBe(money(0));
    await advance(700);
    expect(shown(container)).toBe(money(5835463));
  });

  it("counts from the previous amount when it changes", async () => {
    const { container, rerender } = render(<AnimatedMoney cents={10000} />);
    await advance(800);
    rerender(<AnimatedMoney cents={20000} />);
    await advance(16);
    const between = shown(container);
    expect(between).not.toBe(money(0));
    expect(between).not.toBe(money(20000));
    await advance(800);
    expect(shown(container)).toBe(money(20000));
  });

  it("reserves the width of the final amount from the first frame", () => {
    const { container } = render(<AnimatedMoney cents={5835463} />);
    expect(reserved(container)).toBe(money(5835463));
    expect((container.querySelector("[data-final]") as HTMLElement).className).toContain("invisible");
  });

  it("shows the amount at once with reduced motion", () => {
    setReducedMotion(true);
    const { container } = render(<AnimatedMoney cents={5835463} />);
    expect(shown(container)).toBe(money(5835463));
  });

  it("gives screen readers only the final amount", () => {
    const { container } = render(<AnimatedMoney cents={5835463} />);
    expect((container.querySelector(".sr-only") as HTMLElement).textContent).toBe(money(5835463));
    expect(container.querySelector("[data-count]")?.getAttribute("aria-hidden")).toBe("true");
  });
});
