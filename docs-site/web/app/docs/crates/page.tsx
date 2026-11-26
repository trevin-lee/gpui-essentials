import type { Metadata } from "next";
import Link from "next/link";

import { registry } from "@/lib/registry";

export const metadata: Metadata = { title: "Crates" };

export default function CratesPage() {
  const { crates } = registry();
  return (
    <article className="max-w-5xl py-8">
      <h1 className="text-3xl font-semibold tracking-tight">Crates</h1>
      <p className="mt-2 text-lg text-muted-foreground">
        Bigger pieces built on gpui-core, each its own crate: an editor, a calendar, drag and drop, a
        canvas, databases and an agent&apos;s cards.
      </p>
      <div className="mt-8 grid gap-4 sm:grid-cols-2">
        {crates.map((c) => (
          <Link
            key={c.slug}
            href={`/docs/crates/${c.slug}/`}
            className="overflow-hidden rounded-xl border bg-card transition-colors hover:border-ring"
          >
            {c.pictures[0] && (
              <div className="flex h-56 items-start justify-center overflow-hidden border-b bg-stage">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src={`/img/crates/${c.slug}/${c.pictures[0].file}`}
                  alt=""
                  loading="lazy"
                  className="w-full object-cover object-top"
                />
              </div>
            )}
            <div className="p-4">
              <div className="font-medium">{c.name}</div>
              <p className="mt-1 text-sm text-muted-foreground">{c.about}</p>
            </div>
          </Link>
        ))}
      </div>
    </article>
  );
}
