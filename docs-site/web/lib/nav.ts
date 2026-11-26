import "server-only";

import { byFamily } from "@/lib/registry";

export type NavGroup = { title: string; items: { title: string; href: string }[] };

/** The sidebar: getting started, then the components by family. */
export function nav(): NavGroup[] {
  return [
    {
      title: "Getting started",
      items: [
        { title: "Introduction", href: "/docs/" },
        { title: "Components", href: "/docs/components/" },
      ],
    },
    ...byFamily().map(({ family, components }) => ({
      title: family,
      items: components.map((c) => ({ title: c.name, href: `/docs/components/${c.slug}/` })),
    })),
  ];
}
