import Link from "next/link";

import { byFamily, pictures } from "@/lib/registry";

/** Every component by family, a card each with its first variant's picture. */
export function ComponentGrid() {
  return (
    <>
      {byFamily().map(({ family, components }) => (
        <section key={family} className="mt-10">
          <h2 className="mb-4 text-lg font-semibold tracking-tight">{family}</h2>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {components.map((c) => {
              const p = c.variants[0] ? pictures(c.slug, c.variants[0].slug) : undefined;
              return (
                <Link
                  key={c.slug}
                  href={`/docs/components/${c.slug}/`}
                  className="group overflow-hidden rounded-xl border bg-card transition-colors hover:border-ring"
                >
                  <div className="flex h-40 items-center justify-center overflow-hidden border-b bg-stage">
                    {p?.size && (
                      <>
                        {/* eslint-disable-next-line @next/next/no-img-element */}
                        <img src={p.light} alt="" loading="lazy" className="max-h-full max-w-full object-contain dark:hidden" />
                        {/* eslint-disable-next-line @next/next/no-img-element */}
                        <img src={p.dark} alt="" loading="lazy" className="hidden max-h-full max-w-full object-contain dark:block" />
                      </>
                    )}
                  </div>
                  <div className="p-4">
                    <div className="flex items-center gap-2 font-medium">
                      {c.name}
                    </div>
                    <p className="mt-1 text-sm text-muted-foreground">{c.about}</p>
                  </div>
                </Link>
              );
            })}
          </div>
        </section>
      ))}
    </>
  );
}
