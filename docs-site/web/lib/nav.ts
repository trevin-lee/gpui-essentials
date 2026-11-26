import "server-only";

import { byFamily, registry } from "@/lib/registry";

export type NavGroup = { title: string; items: { title: string; href: string }[] };

/** The sidebar: getting started, the crates built on gpui-core, then the components by
 *  family. */
export function nav(): NavGroup[] {
  return [
    {
      title: "Getting started",
      items: [
        { title: "Introduction", href: "/docs/" },
        { title: "Components", href: "/docs/components/" },
        { title: "Crates", href: "/docs/crates/" },
      ],
    },
    {
      title: "Crates",
      items: registry().crates.map((c) => ({ title: c.name, href: `/docs/crates/${c.slug}/` })),
    },
    ...byFamily().map(({ family, components }) => ({
      title: family,
      items: components.map((c) => ({ title: c.name, href: `/docs/components/${c.slug}/` })),
    })),
  ];
}
