import type { Metadata } from "next";
import localFont from "next/font/local";
import { WalletProviderRoot } from "@/components/WalletProviderRoot";
import "@solana/wallet-adapter-react-ui/styles.css";
import "./globals.css";

const cause = localFont({
  src: "../public/fonts/Cause/Cause-Medium.ttf",
  weight: "500",
  display: "swap",
  variable: "--font-cause"
});

export const metadata: Metadata = {
  title: "Shadow SDK Console",
  description: "Private Solana intent execution console",
  icons: {
    icon: "/logo/logo.png",
    shortcut: "/logo/logo.png",
    apple: "/logo/logo.png"
  }
};

export default function RootLayout({
  children
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={cause.variable}>
        <WalletProviderRoot>{children}</WalletProviderRoot>
      </body>
    </html>
  );
}
