import { describe, expect, it } from "vitest";
import type { MissionTask } from "@/lib/ipc";
import { toInputs } from "./PlanEditor";

const task = (id: string, key: string, deps: string[] = []): MissionTask => ({
  id,
  missionId: "m",
  position: 0,
  key,
  title: key.toUpperCase(),
  description: "d",
  acceptance: "",
  profileId: "builtin:backend",
  dependsOn: deps,
  status: "pending",
  runId: null,
  worktreePath: null,
  worktreeBranch: null,
  attempts: 0,
  feedback: null,
  integratedAt: null,
  createdAt: 0,
  updatedAt: 0,
  baseCommit: null,
  reviewRunId: null,
  reviewVerdict: null,
  reviewSummary: null,
  providerOverride: null,
});

describe("toInputs", () => {
  it("maps dependency ids back to keys for editing", () => {
    const inputs = toInputs([task("id-a", "api"), task("id-b", "ui", ["id-a"])]);
    expect(inputs[1]!.dependsOn).toEqual(["api"]);
    expect(inputs[0]).toMatchObject({ key: "api", title: "API", profileId: "builtin:backend" });
  });
});
