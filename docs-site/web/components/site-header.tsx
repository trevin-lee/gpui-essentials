import Link from "next/link";

import { MobileNav } from "@/components/mobile-nav";
import { ThemeToggle } from "@/components/theme-toggle";
import { nav } from "@/lib/nav";
import { registry } from "@/lib/registry";

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-40 w-full border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="mx-auto flex h-14 max-w-screen-2xl items-center gap-4 px-4 md:px-6">
        <MobileNav groups={nav()} />
        <Link href="/" className="flex items-center gap-2 font-semibold">
          <Logo />
          <span>gpui-essentials</span>
        </Link>
        <nav className="hidden items-center gap-5 text-sm text-muted-foreground md:flex">
          <Link href="/docs/" className="hover:text-foreground">
            Docs
          </Link>
          <Link href="/docs/components/" className="hover:text-foreground">
            Components
          </Link>
          <a
            href="https://trevin-lee.github.io/gpui-essentials/api/gpui_core/"
            className="hover:text-foreground"
          >
            API
          </a>
        </nav>
        <div className="ml-auto flex items-center gap-1">
          <a
            href={registry().repo}
            className="inline-flex size-8 items-center justify-center rounded-md hover:bg-accent"
            aria-label="The source on GitHub"
          >
            <GitHub />
          </a>
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}

function Logo() {
  return (
    <svg viewBox="0 0 24 24" className="size-5" aria-hidden="true">
      <rect x="3" y="3" width="8" height="8" rx="2" fill="currentColor" />
      <rect x="13" y="3" width="8" height="8" rx="2" fill="currentColor" opacity="0.5" />
      <rect x="3" y="13" width="8" height="8" rx="2" fill="currentColor" opacity="0.5" />
      <rect x="13" y="13" width="8" height="8" rx="4" fill="currentColor" />
    </svg>
  );
}

function GitHub() {
  return (
    <svg viewBox="0 0 24 24" className="size-4" fill="currentColor" aria-hidden="true">
      <path d="M12 .3a12 12 0 0 0-3.8 23.38c.6.12.83-.26.83-.57L9 21.07c-3.34.72-4.04-1.61-4.04-1.61-.55-1.39-1.34-1.76-1.34-1.76-1.08-.74.09-.73.09-.73 1.2.09 1.84 1.24 1.84 1.24 1.07 1.83 2.8 1.3 3.49 1 .1-.78.42-1.31.76-1.61-2.67-.3-5.47-1.33-5.47-5.93 0-1.31.47-2.38 1.24-3.22-.14-.3-.54-1.52.1-3.18 0 0 1-.32 3.3 1.23a11.5 11.5 0 0 1 6 0c2.28-1.55 3.29-1.23 3.29-1.23.64 1.66.24 2.88.12 3.18a4.65 4.65 0 0 1 1.23 3.22c0 4.61-2.8 5.63-5.48 5.92.42.36.81 1.1.81 2.22l-.01 3.29c0 .31.2.69.82.57A12 12 0 0 0 12 .3" />
    </svg>
  );
}
