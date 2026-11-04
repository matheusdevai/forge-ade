import { describe, expect, it } from "vitest";
import { coverage, fmtKb, fmtMs, fmtUsd, reportedValue } from "./format";

describe("observability format", () => {
  it("never presents unknown data as zero", () => {
    expect(reportedValue({ value: null, reportedRuns: 0, unreportedRuns: 5 }, fmtUsd)).toBe("—");
    expect(reportedValue({ value: 0.0123, reportedRuns: 2, unreportedRuns: 3 }, fmtUsd)).toBe("US$ 0.01");
    expect(coverage({ value: null, reportedRuns: 0, unreportedRuns: 5 })).toBe("não informado pelos CLIs");
    expect(coverage({ value: 1, reportedRuns: 2, unreportedRuns: 3 })).toBe("informado em 2 de 5");
    expect(coverage({ value: 1, reportedRuns: 4, unreportedRuns: 0 })).toBe("informado em todas as 4");
    expect(coverage({ value: null, reportedRuns: 0, unreportedRuns: 0 })).toBe("sem execuções finalizadas");
  });

  it("formats durations, memory and small costs", () => {
    expect(fmtMs(null)).toBe("—");
    expect(fmtMs(850)).toBe("850 ms");
    expect(fmtMs(4200)).toBe("4.2 s");
    expect(fmtMs(125_000)).toBe("2 min 5 s");
    expect(fmtKb(2048)).toBe("2 MB");
    expect(fmtUsd(0.0042)).toBe("US$ 0.0042");
  });
});
