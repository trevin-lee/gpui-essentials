import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";

import { CodeBlock } from "@/components/code-block";
import { ComponentPreview } from "@/components/component-preview";
import { Installation } from "@/components/installation";
import { canRunLive } from "@/lib/live";
import {
  API_DOCS,
  component as find,
  liveSize,
  pictures,
  registry,
  splitComment,
} from "@/lib/registry";

type Props = { params: Promise<{ slug: string }> };

export function generateStaticParams() {
  return registry().components.map((c) => ({ slug: c.slug }));
}

export const dynamicParams = false;

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const c = find((await params).slug);
  return c ? { title: c.name, description: c.about } : {};
}

export default async function ComponentPage({ params }: Props) {
  const c = find((await params).slug);
  if (!c) notFound();
  const live = canRunLive(c.slug);
  const size = live ? liveSize() : undefined;
  const [first, ...rest] = c.variants;

  const preview = (v: (typeof c.variants)[number], autoStart: boolean) => {
    const { body } = splitComment(v.code);
    const p = pictures(c.slug, v.slug);
    return (
      <ComponentPreview
        component={c.slug}
        variant={v.slug}
        name={`${c.name}: ${v.name}`}
        picture={p.size ? { light: p.light, dark: p.dark, ...p.size } : undefined}
        live={live}
        autoStart={autoStart}
        liveSize={size}
        code={<CodeBlock code={body} lang="rust" />}
      />
    );
  };

  return (
    <div className="flex gap-10">
      <article className="min-w-0 max-w-3xl flex-1 py-8">
        <nav className="mb-4 text-sm text-muted-foreground">
          <Link href="/docs/" className="hover:text-foreground">
            Docs
          </Link>
          <span className="mx-1.5">/</span>
          <Link href="/docs/components/" className="hover:text-foreground">
            Components
          </Link>
          <span className="mx-1.5">/</span>
          <span className="text-foreground">{c.name}</span>
        </nav>
        <h1 className="text-3xl font-semibold tracking-tight">{c.name}</h1>
        <p className="mt-2 text-lg text-muted-foreground">{c.about}</p>
        <div className="mt-4 flex gap-2">
          <a
            href={API_DOCS + c.docs}
            className="inline-flex h-6 items-center rounded-md bg-muted px-2 text-xs font-medium hover:bg-accent"
          >
            API reference
          </a>
          {live && (
            <span className="inline-flex h-6 items-center rounded-md bg-muted px-2 text-xs font-medium">
              Runs live
            </span>
          )}
        </div>

        {first && preview(first, true)}
        {first && splitComment(first.code).comment && (
          <p className="text-sm leading-6 text-muted-foreground">{splitComment(first.code).comment}</p>
        )}

        <h2 id="installation" className="mt-12 scroll-mt-20 border-b pb-2 text-2xl font-semibold tracking-tight">
          Installation
        </h2>
        <Installation component={c} repo={registry().repo} />

        <h2 id="usage" className="mt-12 scroll-mt-20 border-b pb-2 text-2xl font-semibold tracking-tight">
          Usage
        </h2>
        <p className="mt-4 leading-7">
          Each example is written with <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">use gpui::*;</code>{" "}
          and <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">use gpui_core::*;</code> in scope. One that
          is an expression has <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">window</code> and{" "}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">cx</code> from the{" "}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-sm">render</code> it&apos;s in; one with state of
          its own is a view. The code is the code that drew the picture{live ? " and runs live" : ""}.
        </p>

        {rest.length > 0 && (
          <h2 id="examples" className="mt-12 scroll-mt-20 border-b pb-2 text-2xl font-semibold tracking-tight">
            Examples
          </h2>
        )}
        {rest.map((v) => {
          const { comment } = splitComment(v.code);
          return (
            <section key={v.slug} id={v.slug} className="mt-8 scroll-mt-20">
              <h3 className="text-xl font-semibold tracking-tight">{v.name}</h3>
              {comment && <p className="mt-2 leading-7 text-muted-foreground">{comment}</p>}
              {preview(v, false)}
            </section>
          );
        })}
      </article>

      <aside className="sticky top-14 hidden h-[calc(100svh-3.5rem)] w-48 shrink-0 overflow-y-auto py-8 text-sm xl:block">
        <div className="mb-2 font-medium">On this page</div>
        {[
          ["installation", "Installation"],
          ["usage", "Usage"],
          ...(rest.length > 0 ? [["examples", "Examples"]] : []),
          ...rest.map((v) => [v.slug, v.name]),
        ].map(([id, title], i) => (
          <a
            key={id}
            href={`#${id}`}
            className={`block py-1 text-muted-foreground hover:text-foreground ${i > 2 ? "pl-3" : ""}`}
          >
            {title}
          </a>
        ))}
      </aside>
    </div>
  );
}
