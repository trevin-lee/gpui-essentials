// The live previews: the GPUI app built from docs-site/live (public/live/, made by
// live/build.sh), loaded once a visit and shared by every preview.
//
// GPUI's web platform opens one window per page, in one canvas it adds to the body, and can't
// open a second. So the site starts it once, keeps its canvas, and moves that canvas into
// whichever preview is live; the window then draws that preview's demo. The other previews
// show their pictures.

/** The components whose previews stay pictures, and why. Every other component's previews can
 *  go live: each was mounted, clicked and tabbed through in headless Chromium. */
const PICTURES_ONLY = new Map<string, string>([
  // ["<component slug>", "why it can't run in a browser yet"],
]);

export const canRunLive = (component: string) => !PICTURES_ONLY.has(component);

/** What live/build.sh's wasm-bindgen glue exports (docs-site/live/src/stage.rs). */
type Bundle = {
  default: (init?: { module_or_path?: string }) => Promise<unknown>;
  stage(dark: boolean, ready: (error?: string) => void): void;
  mount(component: string, variant: string, top: number, height: number): boolean;
  unmount(): void;
  theme(dark: boolean): void;
};

const BASE = "/live/";
/** Set once a visit has started the live previews, so later pages start their first one. */
const STARTED = "gpui-live-started";

let bundle: Bundle | undefined;
let canvas: HTMLCanvasElement | undefined;
let parking: HTMLDivElement | undefined;
let loading: Promise<void> | undefined;
let failure: string | undefined;
let owner: string | undefined;
let version = 0;
const listeners = new Set<() => void>();

function changed() {
  version++;
  for (const listener of listeners) listener();
}

export function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export const snapshot = () => version;

export const liveOwner = () => owner;
export const liveFailure = () => failure;
export const liveLoading = () => loading !== undefined && canvas === undefined && !failure;

export function startedThisVisit() {
  try {
    return sessionStorage.getItem(STARTED) === "1";
  } catch {
    return false;
  }
}

// A bundler would try to resolve `import()` of the glue at build time; it's served as is.
const importGlue = new Function("url", "return import(url)") as (url: string) => Promise<Bundle>;

function fail(error: unknown) {
  failure = error instanceof Error ? error.message : String(error);
  console.error("The live preview stopped:", error);
  if (canvas) {
    canvas.remove();
    canvas = undefined;
  }
  owner = undefined;
  changed();
}

/** Downloads and starts the app, once. */
function load(dark: boolean): Promise<void> {
  loading ??= (async () => {
    changed();
    const glue = await importGlue(`${BASE}live.js`);
    await glue.default({ module_or_path: `${BASE}live_bg.wasm` });
    const before = new Set(document.querySelectorAll("body > canvas"));
    await new Promise<void>((resolve, reject) =>
      glue.stage(dark, (error) => (error ? reject(new Error(error)) : resolve())),
    );
    const added = [...document.querySelectorAll<HTMLCanvasElement>("body > canvas")].find(
      (c) => !before.has(c),
    );
    if (!added) throw new Error("GPUI's canvas didn't appear");
    parking = document.createElement("div");
    parking.style.display = "none";
    document.body.append(parking);
    parking.append(added);
    canvas = added;
    bundle = glue;
    try {
      sessionStorage.setItem(STARTED, "1");
    } catch {}
  })().catch((error) => {
    fail(error);
    throw error;
  });
  return loading;
}

/** Makes the preview `id` live in `stage`, drawing `variant` of `component` in the middle of
 *  `band` (where its picture is in the stage, in CSS pixels; 0 tall for the stage's middle). */
export async function attach(
  id: string,
  stage: HTMLElement,
  component: string,
  variant: string,
  dark: boolean,
  band: { top: number; height: number },
) {
  if (failure) return;
  owner = id;
  changed();
  await load(dark);
  // Another preview may have been asked for while this one loaded.
  if (owner !== id || !canvas || !bundle) return;
  try {
    bundle.theme(dark);
    stage.append(canvas);
    if (!bundle.mount(component, variant, band.top, band.height)) {
      throw new Error(`the live bundle has no ${component}/${variant}`);
    }
  } catch (error) {
    fail(error);
  }
  changed();
}

/** Puts the canvas away if `id` has it, and empties the stage. */
export function detach(id: string) {
  if (owner !== id) return;
  owner = undefined;
  if (canvas && parking && bundle) {
    parking.append(canvas);
    try {
      bundle.unmount();
    } catch (error) {
      fail(error);
    }
  }
  changed();
}

/** Follows the page's theme. */
export function setLiveTheme(dark: boolean) {
  if (!bundle || failure) return;
  try {
    bundle.theme(dark);
  } catch (error) {
    fail(error);
  }
}
