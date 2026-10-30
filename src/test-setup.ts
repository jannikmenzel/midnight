import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

export function setReducedMotion(reduce: boolean) {
  window.matchMedia = ((query: string) => ({ matches: reduce, media: query })) as unknown as typeof window.matchMedia;
}

setReducedMotion(false);

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  setReducedMotion(false);
});
