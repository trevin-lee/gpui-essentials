import Link from "next/link";

import { ComponentGrid } from "@/components/component-grid";
import { registry } from "@/lib/registry";

export default function Home() {
  const { components } = registry();
  return (
    <main className="mx-auto max-w-screen-xl px-4 pb-16 md:px-6">
      <section className="flex flex-col items-center py-16 text-center md:py-24">
        <h1 className="max-w-3xl text-4xl font-semibold tracking-tight md:text-5xl">
          Components for GPUI, drawn natively
        </h1>
        <p className="mt-4 max-w-2xl text-lg text-muted-foreground">
          {components.length} components for Rust apps on GPUI: in your theme, and yours to copy.
          Each one here is drawn by GPUI, and you can run them live in your browser.
        </p>
        <div className="mt-8 flex gap-3">
          <Link
            href="/docs/"
            className="inline-flex h-9 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90"
          >
            Get started
          </Link>
          <Link
            href="/docs/components/"
            className="inline-flex h-9 items-center rounded-md border px-4 text-sm font-medium hover:bg-accent"
          >
            Browse components
          </Link>
        </div>
      </section>
      <ComponentGrid />
    </main>
  );
}
