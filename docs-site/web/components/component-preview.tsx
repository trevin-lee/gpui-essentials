"use client";

import { LoaderCircle, Play, Square } from "lucide-react";
import { useTheme } from "next-themes";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";

import {
  attach,
  detach,
  liveFailure,
  liveLoading,
  liveOwner,
  setLiveTheme,
  snapshot,
  startedThisVisit,
  subscribe,
} from "@/lib/live";

type Picture = { light: string; dark: string; width: number; height: number };

/** The renderer draws at the screen's scale: 1× on CI's runners, 2× on a Retina Mac. */
function cssSize(p: Picture) {
  const scale = p.width > 1000 ? 2 : 1;
  return { width: p.width / scale, height: p.height / scale };
}

/** The least height of a preview's picture area. */
const MIN_HEIGHT = 280;
/** Room added below the picture's area while it's live, for a menu, a popover or a calendar
 *  the demo opens. */
const ROOM = 200;

/**
 * One variant: its picture or, when live, the demo running in GPUI; and its code. Like
 * shadcn/ui's previews, with Preview and Code tabs.
 */
export function ComponentPreview({
  component,
  variant,
  name,
  picture,
  live,
  autoStart,
  liveSize,
  code,
}: {
  component: string;
  variant: string;
  name: string;
  picture?: Picture;
  /** Whether this preview can go live. */
  live: boolean;
  /** Goes live by itself on a page load, once the visit has started a live preview. */
  autoStart: boolean;
  /** The live bundle's download, for the button's note ("3.9 MB"). */
  liveSize?: string;
  code: React.ReactNode;
}) {
  const id = `${component}/${variant}`;
  const [tab, setTab] = useState<"preview" | "code">("preview");
  const stage = useRef<HTMLDivElement>(null);
  const area = useRef<HTMLDivElement>(null);
  const { resolvedTheme } = useTheme();
  const dark = resolvedTheme === "dark";
  useSyncExternalStore(subscribe, snapshot, snapshot);
  const isLive = liveOwner() === id;
  const loading = isLive && liveLoading();
  const failure = liveFailure();

  // Where the picture is drawn in the preview: the live demo is drawn there too.
  const band = () => {
    const shown = [...(area.current?.querySelectorAll("img") ?? [])].find((i) => i.offsetHeight > 0);
    return shown ? { top: shown.offsetTop, height: shown.offsetHeight } : { top: 0, height: 0 };
  };

  const start = (isDark = dark) => {
    if (stage.current) void attach(id, stage.current, component, variant, isDark, band());
  };

  useEffect(() => {
    if (live && autoStart && startedThisVisit() && !liveOwner() && stage.current) {
      start(document.documentElement.classList.contains("dark"));
    }
    return () => detach(id);
  }, [id, component, variant, live, autoStart]);

  useEffect(() => {
    if (isLive) setLiveTheme(dark);
  }, [isLive, dark]);

  const size = picture ? cssSize(picture) : undefined;

  return (
    <div className="group relative my-4">
      <div className="flex items-center justify-between border-b">
        <div role="tablist" aria-label={name} className="flex gap-4">
          {(["preview", "code"] as const).map((t) => (
            <button
              key={t}
              role="tab"
              type="button"
              aria-selected={tab === t}
              onClick={() => setTab(t)}
              className={`-mb-px border-b-2 px-1 pb-2 text-sm font-medium capitalize transition-colors ${
                tab === t
                  ? "border-foreground text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              {t}
            </button>
          ))}
        </div>
      </div>

      <div className={tab === "preview" ? "mt-4" : "hidden"}>
        <div className="relative overflow-hidden rounded-lg border bg-stage">
          <div
            ref={area}
            className="relative flex items-center justify-center"
            style={{ minHeight: Math.max(MIN_HEIGHT, size?.height ?? 0) }}
          >
          {picture && size ? (
            <>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src={picture.light}
                width={size.width}
                height={size.height}
                alt={`${name}, drawn by GPUI`}
                className={`h-auto max-w-full dark:hidden ${isLive && !loading ? "invisible" : ""}`}
                loading="lazy"
              />
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src={picture.dark}
                width={size.width}
                height={size.height}
                alt={`${name}, drawn by GPUI`}
                className={`hidden h-auto max-w-full dark:block ${isLive && !loading ? "dark:invisible" : ""}`}
                loading="lazy"
              />
            </>
          ) : (
            <p className="text-sm text-muted-foreground">No picture yet.</p>
          )}
          </div>
          {isLive && <div style={{ height: ROOM }} />}
          <div
            ref={stage}
            className={`absolute inset-0 ${isLive && !loading ? "" : "pointer-events-none"}`}
          />
          {live && (
            <div className="absolute top-2 right-2 flex items-center gap-2">
              {isLive && !loading && (
                <span className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:text-emerald-400">
                  Live
                </span>
              )}
              {!failure && (
                <button
                  type="button"
                  onClick={() => (isLive ? detach(id) : start())}
                  disabled={loading}
                  title={
                    isLive
                      ? "Back to the picture"
                      : liveSize
                        ? `Run it here, in GPUI on WebGPU or WebGL2. The first one downloads ${liveSize}.`
                        : "Run it here, in GPUI on WebGPU or WebGL2."
                  }
                  className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background/80 px-2.5 text-xs font-medium shadow-xs backdrop-blur hover:bg-accent disabled:opacity-70"
                >
                  {loading ? (
                    <>
                      <LoaderCircle className="size-3.5 animate-spin" /> Loading{liveSize ? ` ${liveSize}` : ""}
                    </>
                  ) : isLive ? (
                    <>
                      <Square className="size-3" /> Stop
                    </>
                  ) : (
                    <>
                      <Play className="size-3" /> Run live
                    </>
                  )}
                </button>
              )}
            </div>
          )}
        </div>
        {live && failure && (
          <p className="mt-2 text-xs text-muted-foreground">
            The live preview can't run in this browser ({failure}); the picture is what GPUI drew.
          </p>
        )}
      </div>

      <div className={tab === "code" ? "mt-4" : "hidden"}>{code}</div>
    </div>
  );
}
