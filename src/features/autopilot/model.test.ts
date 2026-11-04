import { describe, expect, it } from "vitest";
import { actionSummary, duration, triggerSummary } from "./model";

describe("autopilot model", () => {
  it("summarizes triggers", () => {
    expect(triggerSummary({ trigger: "manual", intervalMin: null, dailyTime: null, daysMask: 127 })).toBe("Só manualmente");
    expect(triggerSummary({ trigger: "interval", intervalMin: 30, dailyTime: null, daysMask: 127 })).toBe("A cada 30 min");
    expect(triggerSummary({ trigger: "interval", intervalMin: 120, dailyTime: null, daysMask: 127 })).toBe("A cada 2 h");
    expect(triggerSummary({ trigger: "interval", intervalMin: 2880, dailyTime: null, daysMask: 127 })).toBe("A cada 2 dia(s)");
    expect(triggerSummary({ trigger: "daily", intervalMin: null, dailyTime: "09:00", daysMask: 127 })).toBe("todo dia às 09:00");
    expect(triggerSummary({ trigger: "daily", intervalMin: null, dailyTime: "08:30", daysMask: 0b0011111 })).toBe("dias úteis às 08:30");
    expect(triggerSummary({ trigger: "daily", intervalMin: null, dailyTime: "10:00", daysMask: 0b1000001 })).toBe("Seg, Dom às 10:00");
    expect(triggerSummary({ trigger: "commit", intervalMin: null, dailyTime: null, daysMask: 127 })).toBe("A cada novo commit");
  });

  it("summarizes actions and durations", () => {
    expect(actionSummary({ type: "agent", profileId: "p", prompt: "Revise", permission: "read", useWorktree: false })).toBe("Agente (leitura): Revise");
    expect(actionSummary({ type: "command", command: "npm test" })).toBe("Comando: npm test");
    expect(duration(0, 65_000)).toBe("1min 5s");
    expect(duration(0, null)).toBe("—");
  });
});
