import { describe, expect, it } from "vitest";
import { parseTags, scopeNeedsCrossProjectConsent } from "./model";

describe("memory model", () => {
  it("parses tags", () => {
    expect(parseTags("API, #db,  api ,\nDeploy,,")).toEqual(["api", "db", "deploy"]);
    expect(parseTags("")).toEqual([]);
  });

  it("requires consent only for cross-project scopes", () => {
    expect(scopeNeedsCrossProjectConsent({ scope: "global", workspaceId: null })).toBe(true);
    expect(scopeNeedsCrossProjectConsent({ scope: "agent", workspaceId: null })).toBe(true);
    expect(scopeNeedsCrossProjectConsent({ scope: "agent", workspaceId: "w" })).toBe(false);
    expect(scopeNeedsCrossProjectConsent({ scope: "workspace", workspaceId: "w" })).toBe(false);
    expect(scopeNeedsCrossProjectConsent({ scope: "mission", workspaceId: null })).toBe(false);
  });
});
