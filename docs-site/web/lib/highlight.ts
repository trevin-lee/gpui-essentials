import "server-only";

import { codeToHtml } from "shiki";

/** Code as HTML, in GitHub's light colours, with its dark ones as variables for `.dark`. */
export function highlight(code: string, lang: "rust" | "toml" | "sh"): Promise<string> {
  return codeToHtml(code, {
    lang,
    themes: { light: "github-light", dark: "github-dark" },
    defaultColor: "light",
  });
}
