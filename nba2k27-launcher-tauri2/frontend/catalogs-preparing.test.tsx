import { describe, expect, it } from "vitest";
import { catalogsPreparing, type ArenaStatus } from "./NativeArena";

const status = (p: Partial<ArenaStatus>): ArenaStatus => ({
  phase: "waiting-for-selection",
  requestedId: "x",
  activeId: "default",
  revision: 1,
  appliedRevision: 0,
  message: "",
  ...p,
});

describe("catalogsPreparing", () => {
  it("lists requested catalogs until their current request is applied", () => {
    expect(
      catalogsPreparing({
        arena: status({ phase: "preparing" }),
        ball: status({ appliedRevision: 1, phase: "ready" }),
        scoreboard: status({}),
      }),
    ).toEqual(["arena", "scoreboard"]);
  });
  it("ignores unrequested, applied and failed catalogs", () => {
    expect(
      catalogsPreparing({
        arena: status({ revision: 0 }),
        ball: status({ phase: "held" }),
        scoreboard: status({ phase: "error" }),
      }),
    ).toEqual([]);
    expect(catalogsPreparing({})).toEqual([]);
    // A newer request after an applied one is preparing again.
    expect(
      catalogsPreparing({ arena: status({ revision: 2, appliedRevision: 1, phase: "ready" }) }),
    ).toEqual(["arena"]);
  });
});
