"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import type { NavGroup } from "@/lib/nav";

export function Sidebar({ groups }: { groups: NavGroup[] }) {
  const pathname = usePathname();
  return (
    <aside className="sticky top-14 hidden h-[calc(100svh-3.5rem)] w-56 shrink-0 overflow-y-auto py-6 pr-2 md:block">
      {groups.map((group) => (
        <div key={group.title} className="mb-5">
          <div className="mb-1 px-2 text-xs font-medium text-muted-foreground">{group.title}</div>
          {group.items.map((item) => {
            const active = pathname === item.href || pathname + "/" === item.href;
            return (
              <Link
                key={item.href}
                href={item.href}
                aria-current={active ? "page" : undefined}
                className={`block rounded-md px-2 py-1 text-sm hover:bg-accent ${
                  active ? "bg-accent font-medium text-accent-foreground" : ""
                }`}
              >
                {item.title}
              </Link>
            );
          })}
        </div>
      ))}
    </aside>
  );
}
