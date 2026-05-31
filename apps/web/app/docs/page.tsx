import type { Metadata } from "next";
import { DocsPage } from "@/components/DocsPage";

export const metadata: Metadata = {
  title: "Shadow SDK Docs",
  description: "Developer documentation for Shadow SDK private Solana intents"
};

export default function Page() {
  return <DocsPage />;
}
