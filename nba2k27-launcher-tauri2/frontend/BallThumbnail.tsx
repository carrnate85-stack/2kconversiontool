import { useEffect, useRef, useState, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { call } from "./api";

type Reply = { state: string; imagePath?: string };
export function ballThumbnailSource(reply: Reply): string | null {
  return reply.state === "ready" && reply.imagePath ? convertFileSrc(reply.imagePath) : null;
}
type Observer = { observe: (element: Element) => void; disconnect: () => void };
export type BallObserver = (visible: (near: boolean) => void) => Observer;
const nearViewport: BallObserver = (visible) => new IntersectionObserver(
  (entries) => visible(entries.some((entry) => entry.isIntersecting)),
  { rootMargin: "200px" },
);
/** Shared by the component and observer/RPC regression: nothing queues offscreen. */
export function observeBallThumbnail(
  element: Element,
  request: () => Promise<Reply>,
  ready: (source: string) => void,
  observe: BallObserver = nearViewport,
) {
  let alive = true, started = false, attempts = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  async function load() {
    try {
      const reply = await request();
      if (!alive) return;
      if (reply.state === "busy" && ++attempts < 8) {
        timer = setTimeout(() => void load(), 250);
      } else {
        const source = ballThumbnailSource(reply);
        if (source) ready(source);
      }
    } catch { /* Optional art keeps its visible placeholder. */ }
  }
  const observer = observe((near) => {
    if (!alive || !near || started) return;
    started = true;
    observer.disconnect();
    void load();
  });
  observer.observe(element);
  return () => { alive = false; observer.disconnect(); clearTimeout(timer); };
}

/** Imported/bundled art wins. Missing game art decodes only near the viewport. */
export function BallThumbnail({ id, gameRoot, image, imageClassName = "blacktop-catalog-preview", placeholder }: {
  id: string; gameRoot: string; image?: string; imageClassName?: string; placeholder?: ReactNode;
}) {
  const target = useRef<HTMLSpanElement>(null);
  const [failed, setFailed] = useState(false);
  const [fallback, setFallback] = useState<string | null>(null);
  useEffect(() => { setFailed(false); setFallback(null); }, [id, image, gameRoot]);
  useEffect(() => {
    if ((image && !failed) || id === "current" || !gameRoot || !target.current || typeof IntersectionObserver === "undefined") return;
    return observeBallThumbnail(target.current,
      () => call<Reply>("previews:ball-thumbnail", { id }), setFallback);
  }, [id, gameRoot, image, failed]);
  const src = fallback || (!failed ? image : undefined);
  return <span ref={target} className="blacktop-ball-thumbnail">
    {src ? <img className={imageClassName} src={src} alt="" loading="lazy" decoding="async"
      onError={() => { if (fallback) setFallback(null); else setFailed(true); }} />
      : placeholder ?? <span className="blacktop-catalog-ball">{id === "current" ? "CURRENT" : "●"}</span>}
  </span>;
}
