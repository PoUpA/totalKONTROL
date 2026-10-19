import { describe, expect, it } from "vitest";
import type { Profile } from "../../types";
import { getAction, noteName, removeBank, setAction, summarize, swapBanks } from "./profile";

const t = (key: string, vars?: Record<string, string | number>) => `${key}${vars ? JSON.stringify(vars) : ""}`;

/** Three banks; bank 2 jumps to bank 0 and bank 0 to bank 2, from a pad, a button and the XY touch. */
function profile(): Profile {
  return {
    version: 1,
    name: "P",
    startBank: 2,
    banks: [
      { name: "A", pads: { "1": { type: "switchBank", bank: 2 } } },
      { name: "B", buttons: { setting: { type: "switchBank", bank: 2 } } },
      { name: "C", xyTouch: { type: "switchBank", bank: 0 } },
    ],
  };
}

describe("swapBanks", () => {
  it("swaps the banks and every reference to them", () => {
    const swapped = swapBanks(profile(), 0, 2);
    expect(swapped.banks.map((b) => b.name)).toEqual(["C", "B", "A"]);
    expect(swapped.startBank).toBe(0);
    expect(swapped.banks[2].pads?.["1"]).toEqual({ type: "switchBank", bank: 0 });
    expect(swapped.banks[1].buttons?.setting).toEqual({ type: "switchBank", bank: 0 });
    expect(swapped.banks[0].xyTouch).toEqual({ type: "switchBank", bank: 2 });
  });
});

describe("removeBank", () => {
  it("shifts later references down", () => {
    const removed = removeBank(profile(), 1);
    expect(removed.banks.map((b) => b.name)).toEqual(["A", "C"]);
    expect(removed.banks[0].pads?.["1"]).toEqual({ type: "switchBank", bank: 1 });
    expect(removed.startBank).toBe(1);
  });

  it("leaves references to the removed bank past the end, for validation to flag", () => {
    const removed = removeBank(profile(), 2);
    expect(removed.banks).toHaveLength(2);
    expect(removed.banks[0].pads?.["1"]).toEqual({ type: "switchBank", bank: 2 });
    expect(removed.startBank).toBe(0);
  });
});

describe("actions", () => {
  it("sets and clears an action on a control", () => {
    const bank = setAction({ name: "A" }, { kind: "pad", pad: 3 }, { type: "note", channel: 1, note: 38 });
    expect(getAction(bank, { kind: "pad", pad: 3 })).toEqual({ type: "note", channel: 1, note: 38 });
    expect(getAction(setAction(bank, { kind: "pad", pad: 3 }, undefined), { kind: "pad", pad: 3 })).toBeUndefined();
  });

  it("summarizes actions", () => {
    expect(summarize({ type: "note", channel: 10, note: 36 }, [], t)).toContain("C1");
    expect(summarize({ type: "switchProfile", profile: "live.json" }, [], t)).toBe("⇄ live");
    expect(summarize({ type: "key", keys: "ctrl+s" }, [], t)).toBe("⌨ ctrl+s");
  });

  it("names notes with middle C as C3", () => {
    expect(noteName(60)).toBe("C3");
    expect(noteName(0)).toBe("C-2");
    expect(noteName(127)).toBe("G8");
  });
});
