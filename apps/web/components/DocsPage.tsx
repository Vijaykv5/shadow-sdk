"use client";

import {
  Activity,
  BookOpen,
  Boxes,
  Braces,
  ExternalLink,
  FileJson,
  RadioTower,
  Rocket,
  Search,
  ShieldCheck,
  Terminal,
  Wallet,
  X
} from "lucide-react";
import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";

const navGroups = [
  {
    title: "Getting started",
    icon: <BookOpen className="h-5 w-5" aria-hidden="true" />,
    items: [
      {
        label: "What is Shadow?",
        href: "#what-is-shadow",
        keywords: "private Solana intents payload hash off-chain relayer devnet"
      },
      { label: "Quick start", href: "#quick-start", keywords: "connect wallet create vault first intent" },
      { label: "Create vault", href: "#create-vault", keywords: "owner wallet PDA ephemeral authority" },
      { label: "Submit intent", href: "#submit-intent", keywords: "payload hash mock transfer perps order" },
      { label: "Execute intent", href: "#execute-intent", keywords: "queue relayer devnet commitment route" }
    ]
  },
  {
    title: "Developers",
    icon: <Terminal className="h-5 w-5" aria-hidden="true" />,
    items: [
      { label: "Anchor program", href: "#anchor-program", keywords: "stealth vault program IDL PDA accounts" },
      { label: "TypeScript helpers", href: "#typescript-helpers", keywords: "lib shadow PDA hash transaction account read" },
      { label: "CLI workflow", href: "#cli-workflow", keywords: "cargo terminal config submit inspect" },
      { label: "Relayer service", href: "#relayer-service", keywords: "off-chain payload execute queued work" }
    ]
  },
  {
    title: "References",
    icon: <Boxes className="h-5 w-5" aria-hidden="true" />,
    items: [
      { label: "Program IDL", href: "#program-idl", keywords: "Anchor JSON devnet uploaded interface" },
      { label: "Architecture", href: "#architecture", keywords: "workspace program crate CLI relayer web examples" },
      { label: "Examples", href: "#examples", keywords: "mock intent system transfer perps order payloads" }
    ]
  }
];

const docCards = [
  {
    id: "quick-start",
    icon: <Rocket className="h-6 w-6" aria-hidden="true" />,
    title: "Quick start",
    description: "Connect a wallet, create a stealth vault, then submit your first hashed execution intent."
  },
  {
    id: "create-vault",
    icon: <ShieldCheck className="h-6 w-6" aria-hidden="true" />,
    title: "Create vault",
    description: "Bind an owner wallet to an ephemeral authority that can commit private payload hashes."
  },
  {
    id: "submit-intent",
    icon: <Braces className="h-6 w-6" aria-hidden="true" />,
    title: "Submit intent",
    description: "Compose mock executions, system transfers, or perps orders while keeping payload data off-chain."
  },
  {
    id: "execute-intent",
    icon: <RadioTower className="h-6 w-6" aria-hidden="true" />,
    title: "Execute intent",
    description: "Queue payloads locally, verify them against devnet commitments, then execute through the relayer."
  }
];

const developerSections = [
  {
    id: "anchor-program",
    title: "Anchor program",
    text: "The stealth-vault program owns vault and execution intent PDAs. The checked-in Anchor IDL mirrors the devnet deployment."
  },
  {
    id: "typescript-helpers",
    title: "TypeScript helpers",
    text: "The web app uses typed helpers in lib/shadow.ts for PDA derivation, payload hashing, transaction creation, and account reads."
  },
  {
    id: "cli-workflow",
    title: "CLI workflow",
    text: "The Rust CLI gives you the same flow from the terminal: initialize config, submit intent hashes, and inspect accounts."
  },
  {
    id: "relayer-service",
    title: "Relayer service",
    text: "The relayer stores payloads off-chain, checks them against the committed hash, and executes queued work when requested."
  }
];
const PROGRAM_ID = "3Nz8wUHewqpMuceSLnoeTMyPLaDt9kNzsVMWTCeVMD6M";
const PROGRAM_SOLSCAN_URL = `https://solscan.io/account/${PROGRAM_ID}?cluster=devnet`;

export function DocsPage() {
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const normalizedQuery = searchQuery.trim().toLowerCase();
  const allNavItems = useMemo(
    () =>
      navGroups.flatMap((group) =>
        group.items.map((item) => ({
          ...item,
          group: group.title
        }))
      ),
    []
  );
  const searchMatches = useMemo(() => {
    if (!normalizedQuery) return [];

    return allNavItems.filter((item) =>
      [item.label, item.group, item.keywords]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery)
    );
  }, [allNavItems, normalizedQuery]);
  const visibleNavGroups = useMemo(() => {
    if (!normalizedQuery) return navGroups;

    return navGroups
      .map((group) => ({
        ...group,
        items: group.items.filter((item) =>
          [item.label, group.title, item.keywords]
            .join(" ")
            .toLowerCase()
            .includes(normalizedQuery)
        )
      }))
      .filter((group) => group.items.length > 0);
  }, [normalizedQuery]);

  function jumpToFirstSearchResult() {
    if (!searchMatches[0]) return;
    window.location.hash = searchMatches[0].href;
  }

  useEffect(() => {
    function focusSearch(event: KeyboardEvent) {
      const target = event.target;
      const isTyping =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement ||
        (target instanceof HTMLElement && target.isContentEditable);

      if (event.key === "/" && !isTyping) {
        event.preventDefault();
        searchInputRef.current?.focus();
      }
    }

    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  return (
    <main className="min-h-screen bg-[#0f1110] text-stone-50 lg:h-screen lg:overflow-hidden">
      <header className="border-b border-stone-800 bg-stone-950/60">
        <div className="mx-auto flex w-full max-w-7xl items-center justify-between px-5 py-5 sm:px-8 lg:px-10">
          <Link
            className="flex items-center gap-3 rounded-md focus-visible:ring-2 focus-visible:ring-lime-200"
            href="/"
          >
            <div className="flex h-10 w-16 items-center justify-center overflow-hidden rounded-md border border-lime-300/30 bg-black">
              <img className="h-8 w-auto" src="/logo/logo.png" alt="Shadow SDK" />
            </div>
            <div>
              <p className="text-sm font-semibold text-lime-200">Shadow SDK</p>
              <p className="text-xs text-stone-400">Developer docs</p>
            </div>
          </Link>
          <Link
            className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md bg-lime-300 px-4 py-3 text-sm font-semibold text-stone-950 transition hover:bg-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
            href="/"
          >
            <Wallet className="h-4 w-4" aria-hidden="true" />
            Launch console
          </Link>
        </div>
      </header>

      <section className="bg-[linear-gradient(rgba(132,204,22,0.035)_1px,transparent_1px),linear-gradient(90deg,rgba(132,204,22,0.035)_1px,transparent_1px)] bg-[size:72px_72px] lg:h-[calc(100vh-81px)] lg:overflow-hidden">
        <div className="mx-auto grid w-full max-w-7xl gap-8 px-5 py-12 sm:px-8 lg:h-full lg:grid-cols-[280px_minmax(0,1fr)_220px] lg:px-10 lg:py-0">
          <aside className="scrollbar-none lg:h-full lg:overflow-y-auto lg:py-16 lg:pr-1">
            <div className="mb-5">
              <label className="sr-only" htmlFor="docs-search">
                Search Shadow docs
              </label>
              <div className="flex items-center gap-2 rounded-md border border-stone-800 bg-stone-950/75 px-3 py-2 text-stone-400 focus-within:border-lime-300/50 focus-within:ring-2 focus-within:ring-lime-200/30">
                <Search className="h-5 w-5" aria-hidden="true" />
                <input
                  className="min-h-9 min-w-0 flex-1 bg-transparent text-sm text-stone-100 outline-none placeholder:text-stone-500"
                  id="docs-search"
                  ref={searchInputRef}
                  type="search"
                  value={searchQuery}
                  placeholder="Search Shadow docs"
                  onChange={(event) => setSearchQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      jumpToFirstSearchResult();
                    }
                  }}
                />
                {searchQuery ? (
                  <button
                    aria-label="Clear docs search"
                    className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-stone-400 transition hover:bg-stone-900 hover:text-stone-100 focus-visible:ring-2 focus-visible:ring-lime-200"
                    type="button"
                    onClick={() => setSearchQuery("")}
                  >
                    <X className="h-4 w-4" aria-hidden="true" />
                  </button>
                ) : (
                  <kbd className="rounded border border-stone-700 px-1.5 py-0.5 text-xs text-stone-500">
                    /
                  </kbd>
                )}
              </div>
              {normalizedQuery ? (
                <div className="mt-3 rounded-md border border-stone-800 bg-stone-950/75 p-2">
                  {searchMatches.length > 0 ? (
                    <div className="grid gap-1">
                      <p className="px-2 py-1 text-xs text-stone-500">
                        {searchMatches.length} match{searchMatches.length === 1 ? "" : "es"}
                      </p>
                      {searchMatches.slice(0, 5).map((item) => (
                        <a
                          className="rounded-md px-2 py-2 text-sm text-stone-300 transition hover:bg-stone-900 hover:text-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200"
                          href={item.href}
                          key={item.href}
                        >
                          <span className="block font-medium text-stone-100">{item.label}</span>
                          <span className="text-xs text-stone-500">{item.group}</span>
                        </a>
                      ))}
                    </div>
                  ) : (
                    <p className="px-2 py-3 text-sm text-stone-500">
                      No docs match "{searchQuery.trim()}".
                    </p>
                  )}
                </div>
              ) : null}
            </div>

            <nav className="space-y-7" aria-label="Shadow SDK documentation">
              {visibleNavGroups.map((group) => (
                <div key={group.title}>
                  <div className="mb-3 flex items-center gap-3 text-sm font-semibold text-stone-100">
                    {group.icon}
                    {group.title}
                  </div>
                  <div className="ml-2 grid gap-1 border-l border-stone-800 pl-5">
                    {group.items.map((item, index) => (
                      <a
                        className={`rounded-sm py-1.5 text-sm transition hover:text-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 ${
                          index === 0 ? "font-semibold text-lime-200" : "text-stone-400"
                        }`}
                        href={item.href}
                        key={item.href}
                      >
                        {item.label}
                      </a>
                    ))}
                  </div>
                </div>
              ))}
            </nav>

            <div className="mt-8 grid gap-2 border-t border-stone-800 pt-5">
              <DocLink href="https://github.com/Vijaykv5/shadow-sdk#readme" label="GitHub README" />
              <DocLink href="https://github.com/Vijaykv5/shadow-sdk/blob/main/docs/architecture/repository-structure.md" label="Repository structure" />
              <DocLink href="https://github.com/Vijaykv5/shadow-sdk/blob/main/idl/stealth_vault.json" label="Anchor IDL" />
            </div>
          </aside>

          <div className="scrollbar-none min-w-0 lg:h-full lg:overflow-y-auto lg:py-16 lg:pr-2">
            <section id="what-is-shadow" className="scroll-mt-8">
              <p className="mb-4 inline-flex rounded-md border border-lime-300/30 bg-lime-300/10 px-3 py-1 text-sm font-medium text-lime-200">
                Developer docs
              </p>
              <h1 className="max-w-3xl text-4xl font-semibold leading-tight tracking-normal text-stone-50 sm:text-5xl">
                What is Shadow SDK?
              </h1>
              <p className="mt-5 max-w-3xl text-lg leading-8 text-stone-300">
                Shadow SDK is a devnet toolkit for private Solana intent execution: commit a payload hash on-chain, keep the full intent off-chain, and let a relayer execute only after the route is ready.
              </p>
            </section>

            <div className="mt-12 grid gap-5 md:grid-cols-2">
              {docCards.map((card) => (
                <section
                  className="min-h-52 scroll-mt-8 rounded-md border border-stone-800 bg-stone-950/75 p-6 transition hover:border-lime-300/35 hover:bg-stone-950"
                  id={card.id}
                  key={card.id}
                >
                  <div className="mb-8 text-lime-200">{card.icon}</div>
                  <h2 className="text-xl font-semibold text-stone-50">{card.title}</h2>
                  <p className="mt-3 text-base leading-7 text-stone-400">{card.description}</p>
                </section>
              ))}
            </div>

            <section className="mt-12 scroll-mt-8 rounded-md border border-stone-800 bg-stone-950/75">
              <div className="border-b border-stone-800 px-5 py-4">
                <h2 className="font-semibold text-stone-50">Core flow</h2>
              </div>
              <div className="grid gap-0 md:grid-cols-3">
                {[
                  ["01", "Create vault", "Owner wallet initializes a PDA vault with an ephemeral executor authority."],
                  ["02", "Commit hash", "The console builds an intent payload and writes only its hash to devnet."],
                  ["03", "Execute privately", "The relayer checks the payload against the hash and submits the execution route."]
                ].map(([step, title, text]) => (
                  <article className="border-t border-stone-800 p-5 md:border-l md:border-t-0 first:md:border-l-0" key={step}>
                    <p className="font-mono text-sm text-lime-200">{step}</p>
                    <h3 className="mt-4 font-semibold text-stone-50">{title}</h3>
                    <p className="mt-2 text-sm leading-6 text-stone-400">{text}</p>
                  </article>
                ))}
              </div>
            </section>

            <div className="mt-12 grid gap-5">
              {developerSections.map((section) => (
                <section
                  className="scroll-mt-8 rounded-md border border-stone-800 bg-stone-950/75 p-6"
                  id={section.id}
                  key={section.id}
                >
                  <h2 className="text-xl font-semibold text-stone-50">{section.title}</h2>
                  <p className="mt-3 max-w-3xl text-sm leading-6 text-stone-400">{section.text}</p>
                </section>
              ))}
            </div>

            <section
              className="mt-12 scroll-mt-8 rounded-md border border-stone-800 bg-stone-950/75 p-6"
              id="program-idl"
            >
              <FileJson className="h-6 w-6 text-lime-200" aria-hidden="true" />
              <h2 className="mt-6 text-xl font-semibold text-stone-50">Program IDL</h2>
              <p className="mt-3 text-sm leading-6 text-stone-400">
                The Anchor IDL is checked into the repo and uploaded on devnet for Anchor tooling.
              </p>
              <a
                className="mt-4 flex min-h-11 items-center justify-between gap-3 rounded-md border border-stone-800 bg-stone-900/70 p-4 font-mono text-sm text-stone-200 transition hover:border-lime-300/40 hover:text-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200"
                href={PROGRAM_SOLSCAN_URL}
                rel="noreferrer"
                target="_blank"
              >
                <span className="break-all">{PROGRAM_ID}</span>
                <ExternalLink className="h-4 w-4 shrink-0" aria-hidden="true" />
              </a>
            </section>

            <section
              className="mt-5 scroll-mt-8 rounded-md border border-stone-800 bg-stone-950/75 p-6"
              id="architecture"
            >
              <h2 className="text-xl font-semibold text-stone-50">Architecture</h2>
              <p className="mt-3 text-sm leading-6 text-stone-400">
                The workspace is split into the Anchor program, Rust SDK crate, CLI, relayer service, web console, examples, and public IDL.
              </p>
            </section>

            <section
              className="mt-5 scroll-mt-8 rounded-md border border-stone-800 bg-stone-950/75 p-6"
              id="examples"
            >
              <h2 className="text-xl font-semibold text-stone-50">Examples</h2>
              <p className="mt-3 text-sm leading-6 text-stone-400">
                Start with mock-intent, system-transfer-intent, and perps-order-intent payloads to test the hashing and relayer flow.
              </p>
            </section>
          </div>

          <aside className="scrollbar-none hidden lg:block lg:h-full lg:overflow-y-auto lg:py-16">
            <div className="border-l border-stone-800 pl-5">
              <div className="mb-4 flex items-center gap-2 text-sm font-semibold text-stone-300">
                <Activity className="h-4 w-4" aria-hidden="true" />
                On this page
              </div>
              {navGroups.flatMap((group) => group.items).slice(0, 7).map((item) => (
                <a
                  className="block py-2 pl-4 text-sm text-stone-400 transition hover:text-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 first:border-l first:border-lime-300 first:font-medium first:text-lime-200"
                  href={item.href}
                  key={item.href}
                >
                  {item.label}
                </a>
              ))}
              <div className="mt-8 rounded-md border border-stone-800 bg-stone-950/75 p-4">
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-stone-500">Program</p>
                <a
                  className="mt-3 block break-all rounded-md font-mono text-sm leading-6 text-stone-200 transition hover:text-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200"
                  href={PROGRAM_SOLSCAN_URL}
                  rel="noreferrer"
                  target="_blank"
                >
                  {PROGRAM_ID}
                </a>
                <p className="mt-3 text-sm text-stone-500">Anchor IDL uploaded on devnet.</p>
              </div>
            </div>
          </aside>
        </div>
      </section>
    </main>
  );
}

function DocLink({ href, label }: { href: string; label: string }) {
  return (
    <a
      className="inline-flex min-h-10 items-center justify-between gap-3 rounded-md px-2 py-2 text-sm text-stone-400 transition hover:bg-stone-900/70 hover:text-stone-100 focus-visible:ring-2 focus-visible:ring-lime-200"
      href={href}
      rel="noreferrer"
      target="_blank"
    >
      {label}
      <ExternalLink className="h-4 w-4" aria-hidden="true" />
    </a>
  );
}
