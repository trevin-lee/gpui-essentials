"use client";

import { Menu, X } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";

import type { NavGroup } from "@/lib/nav";

/** The sidebar's links, behind a button, where there's no room for the sidebar. */
export function MobileNav({ groups }: { groups: NavGroup[] }) {
  const [open, setOpen] = useState(false);
  const pathname = usePathname();
  useEffect(() => setOpen(false), [pathname]);
  return (
    <div className="md:hidden">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="inline-flex size-8 items-center justify-center rounded-md hover:bg-accent"
        aria-label={open ? "Close the menu" : "Open the menu"}
        aria-expanded={open}
      >
        {open ? <X className="size-4" /> : <Menu className="size-4" />}
      </button>
      {open && (
        <div className="fixed inset-x-0 top-14 bottom-0 z-50 overflow-y-auto border-t bg-background px-6 py-4">
          {groups.map((group) => (
            <div key={group.title} className="mb-5">
              <div className="mb-1 text-sm font-medium text-muted-foreground">{group.title}</div>
              {group.items.map((item) => (
                <Link key={item.href} href={item.href} className="block py-1.5 text-base">
                  {item.title}
                </Link>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
