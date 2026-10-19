import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

const hook = vi.hoisted(() => ({
  cursor: 0,
  state: [] as unknown[],
  effects: [] as Array<() => void>,
  call: vi.fn(),
}));
vi.mock("./api", () => ({ call: hook.call }));
vi.mock("react", async (original) => ({
  ...(await original<typeof import("react")>()),
  useState: (initial: unknown) => {
    const index = hook.cursor++;
    if (!(index in hook.state)) hook.state[index] = initial;
    return [
      hook.state[index],
      (next: unknown) => {
        hook.state[index] = next;
      },
    ];
  },
  useEffect: (effect: () => void) => {
    hook.effects.push(effect);
  },
}));
import { GameBuildCompatibility } from "./SettingsTools";
type Card = ReactElement<{
  action: ReactElement<{
    onClick: () => void;
    disabled: boolean;
    children: ReactNode;
  }>;
  children: ReactNode;
}>;
function render() {
  hook.cursor = 0;
  const card = GameBuildCompatibility() as Card;
  // Execute every mounted effect: server rendering alone would miss the old
  // eager scan bug, which happened after a closed Settings details mounted.
  for (const effect of hook.effects.splice(0)) effect();
  return {
    card,
    button: card.props.action.props,
    html: renderToStaticMarkup(card),
  };
}
beforeEach(() => {
  hook.cursor = 0;
  hook.state = [];
  hook.effects = [];
  hook.call.mockReset();
});
const report = {
  build: "test-build",
  buildLabel: "Version 2",
  message: "Verified report",
  features: [
    {
      key: "outfits",
      label: "Outfits",
      status: "Mapped",
      message: "Exact mapping",
      anchors: 1,
      found: 1,
      constants: 0,
      derived: 0,
    },
  ],
};

describe("manual game build compatibility", () => {
  it("mounts and rerenders without a scan even when mounted effects execute", () => {
    const initial = render();
    expect(initial.html).toContain("Not checked.");
    expect(initial.button.children).toBe("Check game build");
    expect(initial.button.disabled).toBe(false);
    render();
    // A remount also leaves the file untouched until the button is clicked.
    hook.state = [];
    render();
    expect(hook.call).not.toHaveBeenCalled();
  });
  it("checks only on an explicit click and preserves loading, report and rescan", async () => {
    let resolve!: (value: typeof report) => void;
    const pending = new Promise<typeof report>((done) => {
      resolve = done;
    });
    hook.call.mockReturnValue(pending);
    render().button.onClick();
    expect(hook.call).toHaveBeenCalledExactlyOnceWith(
      "launcher-tools-compatibility",
      { force: true },
    );
    const loading = render();
    expect(loading.button.disabled).toBe(true);
    expect(loading.button.children).toBe("Checking\u2026");
    resolve(report);
    await pending;
    await Promise.resolve();
    const done = render();
    expect(done.button.children).toBe("Rescan game build");
    expect(done.html).toContain("Verified report");
    expect(done.html).toContain("Exact mapping");
    expect(hook.call).toHaveBeenCalledTimes(1);
    done.button.onClick();
    await pending;
    await Promise.resolve();
    expect(hook.call).toHaveBeenCalledTimes(2);
  });
  it("keeps a failed manual check actionable without automatically retrying", async () => {
    hook.call.mockRejectedValue(new Error("Cannot read game file"));
    render().button.onClick();
    await Promise.resolve();
    await Promise.resolve();
    const failed = render();
    expect(failed.html).toContain('role="alert"');
    expect(failed.html).toContain("Cannot read game file");
    expect(failed.button.disabled).toBe(false);
    render();
    expect(hook.call).toHaveBeenCalledTimes(1);
  });
});
