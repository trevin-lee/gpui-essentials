import { CodeBlock } from "@/components/code-block";
import type { Component } from "@/lib/registry";

/** How to get a component: copied into the host's crate with the gpui CLI, or used from
 *  gpui-core as a dependency. From gpui-core's components.toml, through the registry. */
export function Installation({ component, repo }: { component: Component; repo: string }) {
  const dependency = `[dependencies]\ngpui = { package = "gpui-pre", version = "=0.3.12" }\ngpui-core = { git = "${repo}" }`;
  const { copy } = component;
  if (!copy) {
    return (
      <>
        <p className="mt-4 leading-7">It&apos;s part of gpui-core itself: depend on gpui-core.</p>
        <CodeBlock code={dependency} lang="toml" title="Cargo.toml" className="my-4" />
      </>
    );
  }
  return (
    <>
      <p className="mt-4 leading-7">Copy it into your crate, as code you own, with the gpui CLI:</p>
      <CodeBlock
        code={`cargo install --locked --git ${repo} gpui-cli\ngpui add ${copy.name}`}
        lang="sh"
        className="my-4"
      />
      {copy.needs.length > 0 && (
        <p className="text-sm text-muted-foreground">It also copies {copy.needs.join(", ")}, which it uses.</p>
      )}
      {copy.crates.length > 0 && (
        <p className="mt-1 text-sm text-muted-foreground">
          Add {copy.crates.map((c, i) => (
            <span key={c}>
              {i > 0 && ", "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">{c}</code>
            </span>
          ))}{" "}
          to your crate&apos;s dependencies too.
        </p>
      )}
      <p className="mt-4 leading-7">Or use it from gpui-core, as a dependency:</p>
      <CodeBlock code={dependency} lang="toml" title="Cargo.toml" className="my-4" />
    </>
  );
}
