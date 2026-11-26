import { CopyButton } from "@/components/copy-button";
import { highlight } from "@/lib/highlight";

/** Highlighted code, at build time, with a button that copies it. */
export async function CodeBlock({
  code,
  lang,
  title,
  className = "",
}: {
  code: string;
  lang: "rust" | "toml" | "sh";
  title?: string;
  className?: string;
}) {
  const html = await highlight(code, lang);
  return (
    <div className={`relative overflow-hidden rounded-lg border bg-code ${className}`}>
      {title && (
        <div className="border-b px-4 py-2 font-mono text-xs text-muted-foreground">{title}</div>
      )}
      <CopyButton text={code} className="absolute top-2 right-2" />
      <div
        className="overflow-x-auto p-4 font-mono text-[13px] leading-relaxed [&_pre]:outline-none"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}
