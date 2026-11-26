import type { Metadata } from "next";
import Link from "next/link";

import { CodeBlock } from "@/components/code-block";
import { registry } from "@/lib/registry";

export const metadata: Metadata = { title: "Introduction" };

const h2 = "mt-12 scroll-mt-20 border-b pb-2 text-2xl font-semibold tracking-tight";
const p = "mt-4 leading-7";

export default function Introduction() {
  const { repo } = registry();
  return (
    <article className="max-w-3xl py-8">
      <h1 className="text-3xl font-semibold tracking-tight">Introduction</h1>
      <p className="mt-2 text-lg text-muted-foreground">
        Components for GPUI apps, drawn natively, that work with the keyboard, the mouse and touch.
      </p>
      <p className={p}>
        gpui-essentials is a set of Rust crates for{" "}
        <a href="https://www.gpui.rs" className="font-medium underline underline-offset-4">
          GPUI
        </a>
        . gpui-core holds the components the others build on: a theme, buttons, inputs, menus, popovers,
        sheets, charts and more. Every component on this site is drawn by GPUI itself: the pictures on
        macOS, and the live previews in your browser, on GPUI&apos;s web platform (WebGPU, or WebGL2
        where there&apos;s none).
      </p>
      <p className={p}>
        Like shadcn/ui, a component can be copied into your crate as code you own, with the{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">gpui</code> CLI, or used from
        gpui-core as a dependency.
      </p>

      <h2 className={h2}>Installation</h2>
      <p className={p}>
        None of the crates is on crates.io yet. Take them from the repository, pinned to one commit (the
        same commit for all of them), with GPUI at the version they pin:
      </p>
      <CodeBlock
        lang="toml"
        title="Cargo.toml"
        className="my-4"
        code={`[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "${repo}", rev = "<rev>" }

# The app's window. Without \`font-kit\`, GPUI draws no text on macOS.
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.12", features = ["font-kit"] }`}
      />
      <p className={p}>Or copy a component into your crate with the CLI:</p>
      <CodeBlock
        lang="sh"
        className="my-4"
        code={`cargo install --locked --git ${repo} gpui-cli\ngpui add button`}
      />
      <p className={p}>
        Then call <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">gpui_core::init(cx)</code>{" "}
        when the app starts, and open windows with{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">gpui_core::open_window</code>, which
        puts your view in a root that draws dialogs, sheets and toasts.
      </p>
      <p className={p}>
        <Link href="/docs/components/" className="font-medium underline underline-offset-4">
          Browse the components
        </Link>
        , see the{" "}
        <Link href="/docs/crates/" className="font-medium underline underline-offset-4">
          crates built on them
        </Link>
        , or read the{" "}
        <a href={repo} className="font-medium underline underline-offset-4">
          source on GitHub
        </a>
        .
      </p>
    </article>
  );
}
