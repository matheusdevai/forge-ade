import { describe, expect, it } from "vitest";
import { renderTemplate } from "./library";

describe("renderTemplate", () => {
  it("fills values and marks missing ones", () => {
    expect(renderTemplate("SaaS {{produto}} para {{ publico }}", { produto: " Agenda " })).toBe("SaaS Agenda para não especificado");
  });
});
