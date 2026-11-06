import { afterEach, describe, expect, it } from "vitest";
import { money } from "./format";
import { MASK, customer, isMasked, maskText, setMasked } from "./privacy";

describe("privacy mask", () => {
  afterEach(() => setMasked(false));

  it("shows amounts and names while off", () => {
    expect(isMasked()).toBe(false);
    expect(money(123456)).toMatch(/1.234,56|1,234.56/);
    expect(customer("Acme GmbH")).toBe("Acme GmbH");
  });

  it("hides digits and sign but keeps the currency", () => {
    setMasked(true);
    for (const cents of [123456, -99, 0, 100000000]) {
      const text = money(cents, "EUR", { sign: true });
      expect(text).not.toMatch(/[\d+\-−]/);
      expect(text).toContain(MASK);
      expect(text).toContain("€");
    }
    expect(money(5, "EUR")).toBe(money(500000000, "EUR"));
    expect(money(null)).toBe("–");
  });

  it("hides customer names", () => {
    setMasked(true);
    expect(customer("Acme GmbH")).not.toContain("Acme");
    expect(customer(null)).toBeNull();
    expect(customer("")).toBe("");
  });

  it("masks amounts and known names inside text", () => {
    const text = "RE-2026-0001 über 1.190,00 € von Acme GmbH, 70 % absetzbar am 01.10.2026";
    expect(maskText(text, ["Acme GmbH"])).toBe(text);
    setMasked(true);
    const masked = maskText(text, ["Acme GmbH", "  ", "x"]);
    expect(masked).not.toContain("1.190");
    expect(masked).not.toContain("Acme");
    expect(masked).toContain("RE-2026-0001");
    expect(masked).toContain("70 %");
    expect(masked).toContain("01.10.2026");
    expect(maskText("Paid €1,234.50 by acme gmbh", ["Acme GmbH"])).toBe(`Paid €${MASK} by ${customer("x")}`);
  });

  it("remembers the choice", () => {
    setMasked(true);
    expect(localStorage.getItem("privacy-mask")).toBe("1");
    setMasked(false);
    expect(localStorage.getItem("privacy-mask")).toBe("0");
  });
});
