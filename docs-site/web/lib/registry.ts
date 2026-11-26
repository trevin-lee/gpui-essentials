// The site's content: the Rust registry (docs-site/registry), written as JSON by
// `render --json`, and the pictures the renderer drew. Read at build time only.
import "server-only";

import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

export type Variant = {
  name: string;
  slug: string;
  /** `demo`: one expression; `view`: a view with state of its own. */
  kind: "demo" | "view";
  code: string;
};

export type Component = {
  family: string;
  name: string;
  slug: string;
  about: string;
  /** Its rustdoc page, from the API docs' root. */
  docs: string;
  /** What `gpui add` copies, or null for a component that stays in gpui-core. */
  copy: { name: string; needs: string[]; crates: string[] } | null;
  variants: Variant[];
};

export type Crate = {
  name: string;
  slug: string;
  dir: string;
  lib: string;
  about: string;
  pictures: { file: string; alt: string }[];
};

export type Registry = {
  repo: string;
  /** The colour behind every picture and live preview, gpui-core's theme's background. */
  stage: { light: string; dark: string };
  families: string[];
  components: Component[];
  crates: Crate[];
};

/** A picture's size in pixels, for its box's shape before it loads. */
export type Size = { width: number; height: number };

const ROOT = process.cwd();
const REGISTRY = process.env.GPUI_REGISTRY ?? path.join(ROOT, "data/registry.json");

let cached: Registry | undefined;

export function registry(): Registry {
  if (!cached) {
    if (!fs.existsSync(REGISTRY)) {
      throw new Error(
        `${REGISTRY} is missing: write it with \`render --json\` (see docs-site/README.md)`,
      );
    }
    cached = JSON.parse(fs.readFileSync(REGISTRY, "utf8")) as Registry;
  }
  return cached;
}

/** The components by family, in the registry's family order, each family sorted by name. */
export function byFamily(): { family: string; components: Component[] }[] {
  const { families, components } = registry();
  return families
    .map((family) => ({
      family,
      components: components
        .filter((c) => c.family === family)
        .sort((a, b) => a.name.localeCompare(b.name)),
    }))
    .filter((group) => group.components.length > 0);
}

export function component(slug: string): Component | undefined {
  return registry().components.find((c) => c.slug === slug);
}

/** The width and height of a PNG under public/, from its header, or undefined if it's missing. */
export function pictureSize(src: string): Size | undefined {
  const file = path.join(ROOT, "public", src);
  try {
    const fd = fs.openSync(file, "r");
    const header = Buffer.alloc(24);
    fs.readSync(fd, header, 0, 24, 0);
    fs.closeSync(fd);
    return { width: header.readUInt32BE(16), height: header.readUInt32BE(20) };
  } catch {
    return undefined;
  }
}

/** A variant's pictures, light and dark, as the renderer names them. */
export function pictures(component: string, variant: string) {
  const light = `/img/${component}/${variant}-light.png`;
  const dark = `/img/${component}/${variant}-dark.png`;
  return { light, dark, size: pictureSize(light) };
}

/** The demo file's leading comment, which says what the variant is for, and the code after it. */
export function splitComment(code: string): { comment: string; body: string } {
  const lines = code.split("\n");
  let i = 0;
  const comment: string[] = [];
  while (i < lines.length && lines[i].trimStart().startsWith("//")) {
    comment.push(lines[i].trimStart().replace(/^\/\/\s?/, ""));
    i++;
  }
  return { comment: comment.join(" ").trim(), body: lines.slice(i).join("\n").trim() };
}

/** The API docs, published with the GitHub Pages site. */
export const API_DOCS = "https://trevin-lee.github.io/gpui-essentials/api/";

/** The live bundle's download, gzipped, as "3.9 MB", or undefined if it isn't built. */
export function liveSize(): string | undefined {
  try {
    const wasm = fs.readFileSync(path.join(ROOT, "public/live/live_bg.wasm"));
    const glue = fs.readFileSync(path.join(ROOT, "public/live/live.js"));
    const bytes = zlib.gzipSync(wasm).length + zlib.gzipSync(glue).length;
    return `${(bytes / 1e6).toFixed(1)} MB`;
  } catch {
    return undefined;
  }
}
