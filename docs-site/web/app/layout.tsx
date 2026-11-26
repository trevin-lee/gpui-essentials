import type { Metadata } from "next";
import { GeistMono } from "geist/font/mono";
import { GeistSans } from "geist/font/sans";

import { SiteHeader } from "@/components/site-header";
import { ThemeProvider } from "@/components/theme-provider";
import { registry } from "@/lib/registry";

import "./globals.css";

export const metadata: Metadata = {
  title: {
    default: "gpui-essentials",
    template: "%s - gpui-essentials",
  },
  description:
    "Components for GPUI apps, drawn natively: every one shown here running in your browser, beside its code.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  const { stage } = registry();
  return (
    <html
      lang="en"
      className={`${GeistSans.variable} ${GeistMono.variable}`}
      suppressHydrationWarning
    >
      <head>
        {/* gpui-core's background, behind the pictures and the live previews. */}
        <style>{`:root{--stage:${stage.light}}.dark{--stage:${stage.dark}}`}</style>
      </head>
      <body className="min-h-svh font-sans">
        <ThemeProvider>
          <SiteHeader />
          {children}
        </ThemeProvider>
      </body>
    </html>
  );
}
