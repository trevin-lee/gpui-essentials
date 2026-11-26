import { Sidebar } from "@/components/sidebar";
import { nav } from "@/lib/nav";

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto flex max-w-screen-2xl gap-8 px-4 md:px-6">
      <Sidebar groups={nav()} />
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}
