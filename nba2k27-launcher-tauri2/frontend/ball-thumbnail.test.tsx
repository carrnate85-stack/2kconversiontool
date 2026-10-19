import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { BallThumbnail, ballThumbnailSource, observeBallThumbnail, type BallObserver } from "./BallThumbnail";
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (path: string) => `asset://${path}`, invoke: vi.fn() }));

describe("ball catalog thumbnail fallback", () => {
  it("keeps imported or bundled artwork first and missing/current choices visible", () => {
    const bundled = renderToStaticMarkup(<BallThumbnail id="balls/example.iff" gameRoot="C:\Game" image="./previews/ball.png" />);
    expect(bundled).toContain('src="./previews/ball.png"');
    expect(bundled).toContain('loading="lazy"');
    const missing = renderToStaticMarkup(<BallThumbnail id="balls/example.iff" gameRoot="C:\Game" />);
    expect(missing).toContain("●");
    expect(missing).not.toContain("<img");
    expect(renderToStaticMarkup(<BallThumbnail id="current" gameRoot="C:\Game" />)).toContain("CURRENT");
  });
  it("accepts only a ready decoded file and leaves missing/busy results as placeholders", () => {
    expect(ballThumbnailSource({ state: "ready", imagePath: "C:/Cache/ball.png" })).toBe("asset://C:/Cache/ball.png");
    expect(ballThumbnailSource({ state: "busy", imagePath: "C:/Cache/ball.png" })).toBeNull();
    expect(ballThumbnailSource({ state: "missing" })).toBeNull();
    expect(ballThumbnailSource({ state: "ready" })).toBeNull();
  });
});


it("queues no thumbnail RPC until near the viewport, then queues once and disconnects", async () => {
  let visible!: (near: boolean) => void;
  const observer = { observe: vi.fn(), disconnect: vi.fn() };
  const factory: BallObserver = (callback) => { visible = callback; return observer; };
  const element = {} as Element;
  const request = vi.fn(async () => ({ state: "ready", imagePath: "C:/Cache/ball.png" }));
  const ready = vi.fn();
  const dispose = observeBallThumbnail(element, request, ready, factory);
  expect(observer.observe).toHaveBeenCalledWith(element);
  expect(request).not.toHaveBeenCalled();
  visible(false);
  expect(request).not.toHaveBeenCalled();
  visible(true);
  visible(true);
  await Promise.resolve();
  expect(request).toHaveBeenCalledTimes(1);
  expect(ready).toHaveBeenCalledWith("asset://C:/Cache/ball.png");
  expect(observer.disconnect).toHaveBeenCalledTimes(1);
  dispose();
});

it("disconnects replaced/unmounted tiles and ignores a late RPC reply", async () => {
  let visible!: (near: boolean) => void;
  let resolve!: (value: { state: string; imagePath: string }) => void;
  const observer = { observe: vi.fn(), disconnect: vi.fn() };
  const ready = vi.fn();
  const request = vi.fn(() => new Promise<{ state: string; imagePath: string }>((done) => { resolve = done; }));
  const dispose = observeBallThumbnail({} as Element, request, ready,
    (callback) => { visible = callback; return observer; });
  visible(true);
  dispose();
  resolve({ state: "ready", imagePath: "C:/Cache/old.png" });
  await Promise.resolve();
  expect(ready).not.toHaveBeenCalled();
  expect(observer.disconnect).toHaveBeenCalledTimes(2);
  visible(true);
  expect(request).toHaveBeenCalledTimes(1);
});


it("reuses overlay image sizing and its ball icon placeholder without catalog classes", () => {
  const missing = renderToStaticMarkup(<BallThumbnail id="balls/example.iff" gameRoot="C:/Game"
    imageClassName="" placeholder={<span className="tile-symbol">BALL ICON</span>} />);
  expect(missing).toContain('class="tile-symbol"');
  expect(missing).toContain("BALL ICON");
  expect(missing).not.toContain("blacktop-catalog-ball");
  const image = renderToStaticMarkup(<BallThumbnail id="balls/example.iff" gameRoot="C:/Game"
    image="./previews/ball.png" imageClassName="" />);
  expect(image).not.toContain("blacktop-catalog-preview");
  expect(image).toContain('src="./previews/ball.png"');
});
