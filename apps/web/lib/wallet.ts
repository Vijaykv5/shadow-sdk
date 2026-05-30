import type { PublicKey } from "@solana/web3.js";
import type { Transaction } from "@solana/web3.js";

export type BrowserWallet = {
  isPhantom?: boolean;
  isSolflare?: boolean;
  isBackpack?: boolean;
  isGlow?: boolean;
  publicKey?: PublicKey;
  connect: () => Promise<{ publicKey: PublicKey }>;
  disconnect?: () => Promise<void>;
  signAndSendTransaction?: (transaction: Transaction) => Promise<{ signature: string }>;
  signTransaction?: (transaction: Transaction) => Promise<Transaction>;
};

export type WalletProviderOption = {
  id: string;
  name: string;
  wallet: BrowserWallet;
  installed: boolean;
};

declare global {
  interface Window {
    solana?: BrowserWallet;
    phantom?: {
      solana?: BrowserWallet;
    };
    solflare?: BrowserWallet;
    backpack?: BrowserWallet | { solana?: BrowserWallet };
    glowSolana?: BrowserWallet;
  }
}

export function getBrowserWallet(): BrowserWallet | null {
  return getAvailableWallets()[0]?.wallet ?? null;
}

export function getWalletById(id: string): BrowserWallet | null {
  return getAvailableWallets().find((provider) => provider.id === id)?.wallet ?? null;
}

export function getAvailableWallets(): WalletProviderOption[] {
  if (typeof window === "undefined") return [];

  return uniqueWallets([
    window.phantom?.solana
      ? { id: "phantom", name: "Phantom", wallet: window.phantom.solana, installed: true }
      : null,
    window.solflare
      ? { id: "solflare", name: "Solflare", wallet: window.solflare, installed: true }
      : null,
    backpackWallet()
      ? { id: "backpack", name: "Backpack", wallet: backpackWallet()!, installed: true }
      : null,
    window.glowSolana
      ? { id: "glow", name: "Glow", wallet: window.glowSolana, installed: true }
      : null,
    window.solana
      ? {
          id: window.solana.isPhantom ? "phantom" : "injected",
          name: walletName(window.solana),
          wallet: window.solana,
          installed: true
        }
      : null
  ].concat(scanInjectedWallets()));
}

function uniqueWallets(wallets: Array<WalletProviderOption | null>) {
  const seen = new Set<BrowserWallet>();
  const providers: WalletProviderOption[] = [];

  for (const provider of wallets) {
    if (!provider || seen.has(provider.wallet)) continue;
    seen.add(provider.wallet);
    providers.push(provider);
  }

  return providers;
}

function walletName(wallet: BrowserWallet) {
  if (wallet.isPhantom) return "Phantom";
  if (wallet.isSolflare) return "Solflare";
  if (wallet.isBackpack) return "Backpack";
  if (wallet.isGlow) return "Glow";
  return "Detected Wallet";
}

function backpackWallet() {
  const backpack = window.backpack;
  if (!backpack) return null;
  if (isBrowserWallet(backpack)) return backpack;
  return "solana" in backpack && isBrowserWallet(backpack.solana) ? backpack.solana : null;
}

function scanInjectedWallets(): WalletProviderOption[] {
  const providers: WalletProviderOption[] = [];

  for (const [key, value] of Object.entries(window)) {
    if (!isBrowserWallet(value)) continue;
    providers.push({
      id: `window-${key}`,
      name: walletName(value),
      wallet: value,
      installed: true
    });
  }

  return providers;
}

function isBrowserWallet(value: unknown): value is BrowserWallet {
  return (
    typeof value === "object" &&
    value !== null &&
    "connect" in value &&
    typeof value.connect === "function" &&
    ("signTransaction" in value || "signAndSendTransaction" in value)
  );
}
