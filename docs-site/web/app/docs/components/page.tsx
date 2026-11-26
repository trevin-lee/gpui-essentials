import type { Metadata } from "next";

import { ComponentGrid } from "@/components/component-grid";
import { registry } from "@/lib/registry";

export const metadata: Metadata = { title: "Components" };

export default function ComponentsPage() {
  return (
    <article className="max-w-5xl py-8">
      <h1 className="text-3xl font-semibold tracking-tight">Components</h1>
      <p className="mt-2 text-lg text-muted-foreground">
        All {registry().components.length} of gpui-core&apos;s components, each drawn by GPUI in the light
        and the dark theme beside the code that drew it. The ones marked Live also run here, in your
        browser.
      </p>
      <ComponentGrid />
    </article>
  );
}
