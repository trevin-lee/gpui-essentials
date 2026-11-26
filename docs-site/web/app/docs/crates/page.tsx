import type { Metadata } from "next";

import { CrateGrid } from "@/components/crate-grid";

export const metadata: Metadata = { title: "Crates" };

export default function CratesPage() {
  return (
    <article className="max-w-5xl py-8">
      <h1 className="text-3xl font-semibold tracking-tight">Crates</h1>
      <p className="mt-2 text-lg text-muted-foreground">
        Bigger pieces built on gpui-core, each its own crate: an editor, a calendar, drag and drop, a
        canvas, databases and an agent&apos;s cards.
      </p>
      <div className="mt-8">
        <CrateGrid />
      </div>
    </article>
  );
}
