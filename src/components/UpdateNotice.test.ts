import { describe, expect, it } from "vitest";
import { isNewer } from "./UpdateNotice";

describe("isNewer", () => {
  it("compares versions number by number", () => {
    expect(isNewer("v0.1.10", "0.1.9")).toBe(true);
    expect(isNewer("v0.1.3", "0.1.3")).toBe(false);
    expect(isNewer("v0.1.2", "0.1.3")).toBe(false);
    expect(isNewer("v1.0", "0.9.9")).toBe(true);
  });
});
