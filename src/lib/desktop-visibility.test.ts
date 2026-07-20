import { describe, expect, it } from "vitest";
import {
  ALL_DESKTOPS,
  isOnDesktop,
  isStickyOnCurrentDesktop,
  navigationFor,
  parseDesktopIds,
  serializeDesktopIds,
} from "./desktop-visibility";

// The desktop_id column stores one of three things:
//   ""                    - not assigned to any desktop (unknown / lookup failed)
//   "*"                   - pinned to every desktop
//   "{guid}[,{guid}...]"  - assigned to one or more specific desktops
//
// Before this module existed the rule was open-coded in five places with three
// different behaviours, which is what caused the multi-desktop dimming bug (#3).

const DESKTOP_A = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
const DESKTOP_B = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";
const DESKTOP_C = "{10CA6404-288C-49B6-8AAE-CDC6F59C21E4}";

describe("parseDesktopIds", () => {
  it("returns an empty set for an unassigned sticky", () => {
    expect(parseDesktopIds("")).toEqual(new Set());
  });

  it("returns a single id for a sticky assigned to one desktop", () => {
    expect(parseDesktopIds(DESKTOP_A)).toEqual(new Set([DESKTOP_A]));
  });

  it("splits a comma-separated list into individual ids", () => {
    expect(parseDesktopIds(`${DESKTOP_A},${DESKTOP_B}`)).toEqual(
      new Set([DESKTOP_A, DESKTOP_B]),
    );
  });

  it("treats the all-desktops marker as a normal member", () => {
    expect(parseDesktopIds(ALL_DESKTOPS)).toEqual(new Set([ALL_DESKTOPS]));
  });

  it("ignores whitespace around ids", () => {
    expect(parseDesktopIds(` ${DESKTOP_A} , ${DESKTOP_B} `)).toEqual(
      new Set([DESKTOP_A, DESKTOP_B]),
    );
  });

  it("discards empty segments from a trailing or doubled comma", () => {
    expect(parseDesktopIds(`${DESKTOP_A},,`)).toEqual(new Set([DESKTOP_A]));
  });
});

describe("serializeDesktopIds", () => {
  it("round-trips a parsed value back to storage form", () => {
    const raw = `${DESKTOP_A},${DESKTOP_B}`;
    expect(serializeDesktopIds(parseDesktopIds(raw))).toBe(raw);
  });

  it("serializes an empty set to the unassigned value", () => {
    expect(serializeDesktopIds(new Set())).toBe("");
  });
});

describe("isOnDesktop", () => {
  describe("given a sticky pinned to all desktops", () => {
    it("is on whichever desktop is current", () => {
      expect(isOnDesktop(ALL_DESKTOPS, DESKTOP_A)).toBe(true);
      expect(isOnDesktop(ALL_DESKTOPS, DESKTOP_C)).toBe(true);
    });
  });

  describe("given a sticky assigned to a single desktop", () => {
    it("is on that desktop", () => {
      expect(isOnDesktop(DESKTOP_A, DESKTOP_A)).toBe(true);
    });

    it("is not on a different desktop", () => {
      expect(isOnDesktop(DESKTOP_A, DESKTOP_B)).toBe(false);
    });
  });

  // This is the regression that issue #3 is about: the list filter used
  // split(",").includes(...) while the dimming flag used === , so a sticky on
  // two desktops was listed but rendered as though it lived elsewhere.
  describe("given a sticky assigned to several desktops", () => {
    const assigned = `${DESKTOP_A},${DESKTOP_B}`;

    it("is on the first of its desktops", () => {
      expect(isOnDesktop(assigned, DESKTOP_A)).toBe(true);
    });

    it("is on the second of its desktops", () => {
      expect(isOnDesktop(assigned, DESKTOP_B)).toBe(true);
    });

    it("is not on a desktop it was never assigned to", () => {
      expect(isOnDesktop(assigned, DESKTOP_C)).toBe(false);
    });
  });

  describe("given an unassigned sticky", () => {
    it("is not reported as being on any particular desktop", () => {
      expect(isOnDesktop("", DESKTOP_A)).toBe(false);
    });
  });

  describe("given the current desktop is unknown", () => {
    it("is not reported as being on it", () => {
      expect(isOnDesktop(DESKTOP_A, "")).toBe(false);
    });
  });
});

// Drives the "other-desktop" dimming in the manager. A sticky is shown as
// belonging elsewhere only when we positively know it does.
describe("isStickyOnCurrentDesktop", () => {
  const sticky = (desktop_id: string, pinned = 0) => ({ desktop_id, pinned });

  it("treats a sticky as present when the current desktop is unknown", () => {
    expect(isStickyOnCurrentDesktop(sticky(DESKTOP_A), "")).toBe(true);
  });

  it("treats an unassigned sticky as present rather than dimming it", () => {
    expect(isStickyOnCurrentDesktop(sticky(""), DESKTOP_A)).toBe(true);
  });

  it("treats a pinned sticky as present on every desktop", () => {
    expect(isStickyOnCurrentDesktop(sticky(DESKTOP_B, 1), DESKTOP_A)).toBe(true);
  });

  it("treats an all-desktops sticky as present", () => {
    expect(isStickyOnCurrentDesktop(sticky(ALL_DESKTOPS), DESKTOP_A)).toBe(true);
  });

  it("treats a sticky assigned to this desktop as present", () => {
    expect(isStickyOnCurrentDesktop(sticky(DESKTOP_A), DESKTOP_A)).toBe(true);
  });

  it("treats a sticky assigned only elsewhere as absent", () => {
    expect(isStickyOnCurrentDesktop(sticky(DESKTOP_B), DESKTOP_A)).toBe(false);
  });

  // Regression for #3. The old dimming flag compared desktop_id with === , so a
  // sticky assigned to two desktops was dimmed on both of them.
  it("treats a sticky assigned to several desktops as present on each of them", () => {
    const assigned = `${DESKTOP_A},${DESKTOP_B}`;
    expect(isStickyOnCurrentDesktop(sticky(assigned), DESKTOP_A)).toBe(true);
    expect(isStickyOnCurrentDesktop(sticky(assigned), DESKTOP_B)).toBe(true);
  });

  it("still treats a multi-desktop sticky as absent from an unassigned desktop", () => {
    expect(isStickyOnCurrentDesktop(sticky(`${DESKTOP_A},${DESKTOP_B}`), DESKTOP_C)).toBe(
      false,
    );
  });
});

// Where clicking a sticky in the manager should take you. Windows has no
// documented way to switch desktops directly - the app gets there by activating
// a window that already lives on the target - so this only has to decide
// *which* desktop, if any, the window should be placed on first.
describe("navigationFor", () => {
  describe("given a sticky that already lives on this desktop", () => {
    it("focuses it without travelling", () => {
      expect(navigationFor(DESKTOP_A, DESKTOP_A)).toEqual({ kind: "focus" });
    });
  });

  describe("given a sticky pinned to all desktops", () => {
    it("focuses it without travelling, since it is already here", () => {
      expect(navigationFor(ALL_DESKTOPS, DESKTOP_A)).toEqual({ kind: "focus" });
    });
  });

  describe("given a sticky that lives only somewhere else", () => {
    it("travels to the desktop it lives on", () => {
      expect(navigationFor(DESKTOP_B, DESKTOP_A)).toEqual({
        kind: "travel",
        desktopId: DESKTOP_B,
      });
    });
  });

  // The choice recorded on #11: staying put beats travelling to a desktop the
  // sticky also happens to live on.
  describe("given a sticky on several desktops, one of which is this one", () => {
    it("stays here rather than travelling to its first desktop", () => {
      expect(navigationFor(`${DESKTOP_B},${DESKTOP_A}`, DESKTOP_A)).toEqual({
        kind: "focus",
      });
    });
  });

  describe("given a sticky on several desktops, none of which is this one", () => {
    it("travels to the first desktop it names, so the choice is predictable", () => {
      expect(navigationFor(`${DESKTOP_B},${DESKTOP_C}`, DESKTOP_A)).toEqual({
        kind: "travel",
        desktopId: DESKTOP_B,
      });
    });
  });

  describe("given an unassigned sticky", () => {
    it("focuses it where it is, having nowhere to travel to", () => {
      expect(navigationFor("", DESKTOP_A)).toEqual({ kind: "focus" });
    });
  });

  describe("given the current desktop is unknown", () => {
    it("focuses rather than guessing at a destination", () => {
      expect(navigationFor(DESKTOP_B, "")).toEqual({ kind: "focus" });
    });
  });
});
