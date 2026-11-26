import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";

import { CodeBlock } from "@/components/code-block";
import { API_DOCS, crate as find, pictureSize, registry } from "@/lib/registry";

type Props = { params: Promise<{ slug: string }> };

export function generateStaticParams() {
  return registry().crates.map((c) => ({ slug: c.slug }));
}

export const dynamicParams = false;

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const c = find((await params).slug);
  return c ? { title: c.name, description: c.about } : {};
}

const h2 = "mt-12 scroll-mt-20 border-b pb-2 text-2xl font-semibold tracking-tight";

export default async function CratePage({ params }: Props) {
  const c = find((await params).slug);
  if (!c) notFound();
  const { repo } = registry();
  const readme = `${repo}/tree/main/${c.dir}#readme`;
  return (
    <div className="flex gap-10">
      <article className="min-w-0 max-w-4xl flex-1 py-8">
        <nav className="mb-4 text-sm text-muted-foreground">
          <Link href="/docs/" className="hover:text-foreground">
            Docs
          </Link>
          <span className="mx-1.5">/</span>
          <Link href="/docs/crates/" className="hover:text-foreground">
            Crates
          </Link>
          <span className="mx-1.5">/</span>
          <span className="text-foreground">{c.name}</span>
        </nav>
        <h1 className="text-3xl font-semibold tracking-tight">{c.name}</h1>
        <p className="mt-2 text-lg text-muted-foreground">{c.about}</p>
        <div className="mt-4 flex gap-2">
          <a
            href={`${API_DOCS}${c.lib}/`}
            className="inline-flex h-6 items-center rounded-md bg-muted px-2 text-xs font-medium hover:bg-accent"
          >
            API reference
          </a>
          <a
            href={readme}
            className="inline-flex h-6 items-center rounded-md bg-muted px-2 text-xs font-medium hover:bg-accent"
          >
            README
          </a>
        </div>

        <h2 id="installation" className={h2}>
          Installation
        </h2>
        <p className="mt-4 leading-7">
          Take it from the repository with gpui-core, pinned to the same commit, and GPUI at the
          version they pin:
        </p>
        <CodeBlock
          lang="toml"
          title="Cargo.toml"
          className="my-4"
          code={`[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "${repo}", rev = "<rev>" }
${c.name} = { git = "${repo}", rev = "<rev>" }`}
        />
        <p className="leading-7">
          Its{" "}
          <a href={readme} className="font-medium underline underline-offset-4">
            README
          </a>{" "}
          says how to use it, and its example window shows it all at work.
        </p>

        <h2 id="pictures" className={h2}>
          Pictures
        </h2>
        {c.pictures.map((p) => {
          const src = `/img/crates/${c.slug}/${p.file}`;
          const size = pictureSize(src);
          return (
            <figure key={p.file} className="mt-6">
              <div className="overflow-hidden rounded-lg border bg-stage">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src={src}
                  alt={p.alt}
                  width={size?.width}
                  height={size?.height}
                  loading="lazy"
                  className="h-auto w-full"
                />
              </div>
              <figcaption className="mt-2 text-sm text-muted-foreground">{p.alt}</figcaption>
            </figure>
          );
        })}
      </article>
    </div>
  );
}
