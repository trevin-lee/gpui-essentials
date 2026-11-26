import type { NextConfig } from "next";

// A static site: `next build` writes it to out/, which any static host serves (Vercel here).
// Every page is a folder with an index.html, so its address works with or without a host's
// help.
const config: NextConfig = {
  output: "export",
  trailingSlash: true,
  images: { unoptimized: true },
  reactStrictMode: true,
};

export default config;
