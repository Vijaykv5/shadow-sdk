"use client";

import {
  Activity,
  AlertCircle,
  ArrowRight,
  Check,
  Clipboard,
  Copy,
  FileJson,
  KeyRound,
  Loader2,
  RadioTower,
  RefreshCw,
  Terminal,
  Wallet
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import {
  Connection,
  Transaction
} from "@solana/web3.js";
import {
  clusterRpcUrl,
  createInitializeVaultTransaction,
  createSubmitIntentTransaction,
  DEFAULT_RELAYER_URL,
  deriveIntentPda,
  deriveVaultPda,
  executeQueuedIntentWithRelayer,
  executeIntentWithRelayer,
  fetchExecutionIntentAccount,
  fetchVaultAccount,
  formatPayload,
  getRelayerIntent,
  hashIntentPayload,
  hashPayloadBytes,
  IntentKind,
  IntentPayload,
  prepareTransaction,
  queueIntentWithRelayer,
  QueueItem,
  shortAddress,
  STEALTH_VAULT_PROGRAM_ID,
  submitIntentWithRelayer,
  validatePubkey,
  type ExecutionIntentAccount,
  type Cluster,
  type ExecutionRoute,
  type MagicBlockValidator,
  type RelayerQueuedIntent,
  type VaultAccount
} from "@/lib/shadow";

const QUEUE_KEY = "shadow-sdk.intent-queue";
const MIN_NEW_ACCOUNT_TRANSFER_LAMPORTS = 1_000_000;
const DEVNET_DEPLOY_WALLET = "2eDJJZydDTV4HQmbtX6YwhrdfCW7XU3zms9538HGqkuB";
const DEVNET_EPHEMERAL_AUTHORITY = "2ED9SNAYgWN5WU7sKkhZqp1baytvcKQqoNsTwSPZpuVk";
const ROUTE_OPTIONS: Array<Exclude<ExecutionRoute["kind"], "public_rpc">> = [
  "mock_private_bundle",
  "jito_bundle",
  "magicblock_er",
  "magicblock_per"
];
const MAGICBLOCK_VALIDATOR_OPTIONS: MagicBlockValidator[] = [
  "local_er",
  "devnet_asia",
  "devnet_eu",
  "devnet_us",
  "devnet_tee"
];

function freshNonce() {
  return Math.floor(Date.now() / 1000);
}

type ComposerState = {
  kind: IntentKind;
  nonce: number;
  expiresAt: string;
  mockMessage: string;
  transferTo: string;
  transferLamports: number;
  perpsVenue: "mock" | "drift";
  perpsMarket: string;
  perpsSide: "long" | "short";
  perpsSizeLots: number;
  perpsLimitPrice: number;
  perpsSlippageBps: number;
  perpsReduceOnly: boolean;
  perpsClientOrderId: string;
  routeKind: ExecutionRoute["kind"];
  tipLamports: number;
  magicBlockValidator: MagicBlockValidator;
  commitFrequencyMs: number;
};

type Toast = {
  id: string;
  tone: "success" | "error" | "info";
  title: string;
  message?: string;
};

function createInitialComposer(): ComposerState {
  return {
    kind: "mock_execution",
    nonce: freshNonce(),
    expiresAt: "",
    mockMessage: "hello shadow",
    transferTo: "",
    transferLamports: 1000000,
    perpsVenue: "mock",
    perpsMarket: "SOL-PERP",
    perpsSide: "long",
    perpsSizeLots: 10,
    perpsLimitPrice: 150000000,
    perpsSlippageBps: 50,
    perpsReduceOnly: false,
    perpsClientOrderId: "shadow-demo-1",
    routeKind: "mock_private_bundle",
    tipLamports: 5000,
    magicBlockValidator: "devnet_tee",
    commitFrequencyMs: 30000
  };
}

export function ShadowConsole() {
  const nextStepsRef = useRef<HTMLDivElement | null>(null);
  const wallet = useWallet();
  const { setVisible: setWalletModalVisible } = useWalletModal();
  const [cluster, setCluster] = useState<Cluster>("devnet");
  const [walletAddress, setWalletAddress] = useState("");
  const [owner, setOwner] = useState("");
  const [ephemeralAuthority, setEphemeralAuthority] = useState(DEVNET_EPHEMERAL_AUTHORITY);
  const [executorKeypair, setExecutorKeypair] = useState("~/.config/solana/ephemeral.json");
  const [payloadDir, setPayloadDir] = useState("payloads");
  const [relayerUrl, setRelayerUrl] = useState(DEFAULT_RELAYER_URL);
  const [composer, setComposer] = useState<ComposerState>(() => createInitialComposer());
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [payloadText, setPayloadText] = useState("");
  const [payloadHash, setPayloadHash] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [txState, setTxState] = useState<"idle" | "sending" | "success" | "error">("idle");
  const [txMessage, setTxMessage] = useState("");
  const [vaultAccount, setVaultAccount] = useState<VaultAccount | null>(null);
  const [intentAccount, setIntentAccount] = useState<ExecutionIntentAccount | null>(null);
  const [savedPayloadPath, setSavedPayloadPath] = useState("");
  const [relayerState, setRelayerState] = useState<"idle" | "sending" | "success" | "error">(
    "idle"
  );
  const [relayerMessage, setRelayerMessage] = useState("");
  const [relayerResult, setRelayerResult] = useState<{
    signature: string;
    intent: string;
    vault: string;
    payloadHash: string;
  } | null>(null);
  const [persistedIntent, setPersistedIntent] = useState<RelayerQueuedIntent | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);

  const rpcUrl = clusterRpcUrl(cluster);
  const connection = useMemo(() => new Connection(rpcUrl, "confirmed"), [rpcUrl]);

  useEffect(() => {
    const saved = window.localStorage.getItem(QUEUE_KEY);
    if (saved) {
      setQueue(JSON.parse(saved) as QueueItem[]);
    }
  }, []);

  useEffect(() => {
    window.localStorage.setItem(QUEUE_KEY, JSON.stringify(queue));
  }, [queue]);

  useEffect(() => {
    setPayloadText("");
    setPayloadHash("");
    setIntentAccount(null);
    setPersistedIntent(null);
    setRelayerResult(null);
  }, [composer]);

  useEffect(() => {
    if (!vaultAccount) return;
    nextStepsRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [vaultAccount]);

  useEffect(() => {
    const address = wallet.publicKey?.toBase58() ?? "";
    setWalletAddress(address);
    if (!address) return;

    setOwner(address);
    setEphemeralAuthority((current) => current || DEVNET_EPHEMERAL_AUTHORITY);
    setComposer((current) => ({
      ...current,
      transferTo: current.transferTo || address
    }));
    showToast("success", "Wallet connected", shortAddress(address));
  }, [wallet.publicKey]);

  const ownerValid = validatePubkey(owner);
  const vaultPda = useMemo(() => (ownerValid ? deriveVaultPda(owner) : ""), [owner, ownerValid]);
  const intentPda = useMemo(
    () => (vaultPda ? deriveIntentPda(vaultPda, composer.nonce) : ""),
    [composer.nonce, vaultPda]
  );

  const configText = useMemo(() => {
    return [
      `cluster = "${cluster}"`,
      owner ? `owner = "${owner}"` : `owner = "<OWNER_PUBKEY>"`,
      `executor_keypair = "${executorKeypair}"`,
      `payload = "examples/mock-intent.json"`,
      `payload_dir = "${payloadDir}"`,
      `# web relayer url: ${relayerUrl}`,
      "poll_seconds = 5",
      "max_retries = 3"
    ].join("\n");
  }, [cluster, executorKeypair, owner, payloadDir, relayerUrl]);

  const submitIntentCommand = useMemo(() => {
    const hash = payloadHash || "<PAYLOAD_HASH>";
    return [
      "cargo run -p shadow-cli -- submit-intent \\",
      `  --ephemeral-authority-keypair ${executorKeypair} \\`,
      `  --nonce ${composer.nonce} \\`,
      `  --payload-hash ${hash}`
    ].join("\n");
  }, [composer.nonce, executorKeypair, payloadHash]);

  const runRelayerCommand = useMemo(() => {
    return [
      "cargo run -p shadow-relayer -- serve \\",
      "  --cluster devnet \\",
      `  --executor-keypair ${executorKeypair} \\`,
      "  --bind 127.0.0.1:8787"
    ].join("\n");
  }, [executorKeypair]);

  const deployCommand = useMemo(() => {
    return [
      "solana config set --url devnet",
      `solana airdrop 2 ${DEVNET_DEPLOY_WALLET} --url devnet`,
      "anchor build",
      "anchor deploy --provider.cluster devnet",
      `solana program show ${STEALTH_VAULT_PROGRAM_ID.toBase58()} --url devnet`
    ].join("\n");
  }, []);

  function openWalletPicker() {
    setWalletModalVisible(true);
  }

  async function sendWalletTransaction(transaction: Transaction, signerAddress: string) {
    if (!wallet.signTransaction) throw new Error("Connected wallet does not support transaction signing");
    const prepared = await prepareTransaction(connection, transaction, signerAddress);
    const signed = await wallet.signTransaction(prepared);
    return connection.sendRawTransaction(signed.serialize());
  }

  async function createVaultOnchain() {
    if (!walletAddress || !owner || !ephemeralAuthority) return;
    if (walletAddress !== owner) {
      setTxState("error");
      setTxMessage("Connected wallet must match the owner wallet to create this vault.");
      showToast("error", "Wrong wallet", "Connect the owner wallet before creating the vault.");
      return;
    }

    setTxState("sending");
    setTxMessage("Creating vault...");
    try {
      const signature = await sendWalletTransaction(
        createInitializeVaultTransaction(owner, ephemeralAuthority),
        walletAddress
      );
      setTxState("success");
      setTxMessage(`Vault created: ${signature}`);
      showToast("success", "Vault created", shortAddress(signature));
      await refreshVault();
    } catch (error) {
      setTxState("error");
      const message = error instanceof Error ? error.message : "Failed to create vault";
      setTxMessage(message);
      showToast("error", "Vault creation failed", message);
    }
  }

  async function submitIntentOnchain() {
    if (!walletAddress || !owner) return;

    setTxState("sending");
    setTxMessage("Submitting intent...");
    try {
      const validationError = getComposerValidationError(composer);
      if (validationError) throw new Error(validationError);

      const payload = buildPayload(composer);
      const currentPayloadHash = await hashIntentPayload(payload);
      setPayloadText(formatPayload(payload));
      setPayloadHash(currentPayloadHash);

      const signature =
        walletAddress === ephemeralAuthority
          ? await sendWalletTransaction(
              createSubmitIntentTransaction({
                owner,
                ephemeralAuthority,
                nonce: composer.nonce,
                payloadHash: currentPayloadHash
              }),
              walletAddress
            )
          : (
              await submitIntentWithRelayer({
                relayerUrl,
                owner,
                nonce: composer.nonce,
                payloadHash: currentPayloadHash
              })
            ).signature;
      setTxState("success");
      setTxMessage(`Intent submitted: ${signature}`);
      showToast("success", "Intent submitted", "The hash is now on devnet.");
      await refreshIntent();
    } catch (error) {
      setTxState("error");
      const message = formatSubmitError(error);
      setTxMessage(message);
      showToast("error", "Intent submission failed", message);
    }
  }

  function assignFreshNonce() {
    setComposer((value) => ({ ...value, nonce: freshNonce() }));
    setTxMessage("");
    setTxState("idle");
    setRelayerMessage("");
    setRelayerState("idle");
  }

  async function executeViaRelayer() {
    if (!ownerValid) {
      setRelayerState("error");
      setRelayerMessage("Set a valid owner wallet before calling the relayer.");
      showToast("error", "Owner wallet needed", "Set a valid owner wallet first.");
      return;
    }

    setRelayerState("sending");
    setRelayerMessage("Preparing execution...");
    setRelayerResult(null);

    try {
      const validationError = getComposerValidationError(composer);
      if (validationError) throw new Error(validationError);

      const payload = buildPayload(composer);
      const hash = await hashIntentPayload(payload);
      setPayloadText(formatPayload(payload));
      setPayloadHash(hash);

      const result = await executeIntentWithRelayer({
        relayerUrl,
        owner,
        payload
      });
      setRelayerResult({
        signature: result.signature,
        intent: result.intent,
        vault: result.vault,
        payloadHash: result.payload_hash
      });
      setRelayerState("success");
      setRelayerMessage(`Intent executed: ${result.signature}`);
      showToast("success", "Intent executed", shortAddress(result.signature));
      await refreshIntent();
    } catch (error) {
      setRelayerState("error");
      const message = error instanceof Error ? error.message : "Execution failed";
      setRelayerMessage(message);
      showToast("error", "Execution failed", message);
    }
  }

  async function queueInRelayer() {
    if (!ownerValid) {
      setRelayerState("error");
      setRelayerMessage("Set a valid owner wallet before queueing the payload.");
      showToast("error", "Owner wallet needed", "Set a valid owner wallet first.");
      return;
    }

    setRelayerState("sending");
    setRelayerMessage("Queueing intent...");

    try {
      const validationError = getComposerValidationError(composer);
      if (validationError) throw new Error(validationError);

      const payload = buildPayload(composer);
      const hash = await hashIntentPayload(payload);
      setPayloadText(formatPayload(payload));
      setPayloadHash(hash);

      const queued = await queueIntentWithRelayer({
        relayerUrl,
        owner,
        payload
      });
      setPersistedIntent(queued);
      setRelayerState("success");
      setRelayerMessage(`Intent queued: ${queued.id}`);
      showToast("success", "Intent queued", "Ready for relayer execution.");
    } catch (error) {
      setRelayerState("error");
      const message = error instanceof Error ? error.message : "Failed to queue intent";
      setRelayerMessage(message);
      showToast("error", "Queue failed", message);
    }
  }

  async function refreshPersistedIntent() {
    if (!persistedIntent) return;

    setRelayerState("sending");
    setRelayerMessage("Checking intent status...");

    try {
      const queued = await getRelayerIntent({
        relayerUrl,
        id: persistedIntent.id
      });
      setPersistedIntent(queued);
      setRelayerState("success");
      setRelayerMessage(`Intent status: ${queued.status}`);
      showToast("info", "Status updated", queued.status);
    } catch (error) {
      setRelayerState("error");
      const message = error instanceof Error ? error.message : "Failed to fetch intent status";
      setRelayerMessage(message);
      showToast("error", "Status check failed", message);
    }
  }

  async function executePersistedIntent() {
    if (!persistedIntent) return;

    setRelayerState("sending");
    setRelayerMessage("Executing queued intent...");

    try {
      const queued = await executeQueuedIntentWithRelayer({
        relayerUrl,
        id: persistedIntent.id
      });
      setPersistedIntent(queued);
      setRelayerState(queued.status === "failed" ? "error" : "success");
      setRelayerMessage(
        queued.status === "failed"
          ? queued.error ?? "Execution failed"
          : queued.status === "executed"
            ? "Intent executed"
            : `Intent status: ${queued.status}`
      );
      showToast(
        queued.status === "failed" ? "error" : "success",
        queued.status === "failed" ? "Execution failed" : "Intent executed",
        queued.status === "failed" ? queued.error ?? undefined : undefined
      );
      await refreshIntent();
    } catch (error) {
      setRelayerState("error");
      const message = error instanceof Error ? error.message : "Failed to execute queued intent";
      setRelayerMessage(message);
      showToast("error", "Execution failed", message);
    }
  }

  async function refreshVault() {
    if (!vaultPda) return;
    setVaultAccount(await fetchVaultAccount(connection, vaultPda));
  }

  async function refreshIntent() {
    if (!intentPda) return;
    setIntentAccount(await fetchExecutionIntentAccount(connection, intentPda));
  }

  async function savePayloadToPending() {
    const payload = payloadText || formatPayload(buildPayload(composer));
    const response = await fetch("/api/payloads", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ payload, nonce: composer.nonce, kind: composer.kind })
    });
    const result = (await response.json()) as { path?: string; error?: string };
    if (!response.ok) {
      setTxState("error");
      setTxMessage(result.error ?? "Failed to save payload");
      showToast("error", "Save failed", result.error ?? "Failed to save payload");
      return;
    }
    setSavedPayloadPath(result.path ?? "");
    setTxState("success");
    setTxMessage(`Payload saved to ${result.path}`);
    showToast("success", "Payload saved", result.path);
  }

  async function generatePayload() {
    const validationError = getComposerValidationError(composer);
    if (validationError) {
      setTxState("error");
      setTxMessage(validationError);
      showToast("error", "Check the intent", validationError);
      return;
    }

    const payload = buildPayload(composer);
    const text = formatPayload(payload);
    setPayloadText(text);
    setPayloadHash(await hashIntentPayload(payload));
  }

  async function addToQueue() {
    const validationError = getComposerValidationError(composer);
    if (validationError) {
      setTxState("error");
      setTxMessage(validationError);
      showToast("error", "Check the intent", validationError);
      return;
    }

    const payload = payloadText || formatPayload(buildPayload(composer));
    const hash = payloadHash || (await hashPayloadBytes(payload));
    const item: QueueItem = {
      id: crypto.randomUUID(),
      status: "pending",
      kind: composer.kind,
      nonce: composer.nonce,
      hash,
      payload,
      createdAt: new Date().toISOString()
    };
    setPayloadText(payload);
    setPayloadHash(hash);
    setQueue((items) => [item, ...items]);
  }

  function updateQueue(id: string, status: QueueItem["status"], error?: string) {
    setQueue((items) =>
      items.map((item) => (item.id === id ? { ...item, status, error } : item))
    );
  }

  async function copyText(label: string, text: string) {
    await navigator.clipboard.writeText(text);
    setCopied(label);
    window.setTimeout(() => setCopied(null), 1200);
    showToast("info", "Copied", "Ready to paste.");
  }

  function showToast(tone: Toast["tone"], title: string, message?: string) {
    const id = crypto.randomUUID();
    setToasts((items) => [...items.slice(-3), { id, tone, title, message }]);
    window.setTimeout(() => {
      setToasts((items) => items.filter((item) => item.id !== id));
    }, 4200);
  }

  if (!walletAddress) {
    return (
      <main className="min-h-screen bg-[#0f1110] text-stone-50">
        <ToastRegion toasts={toasts} />
        <section className="mx-auto flex min-h-screen w-full max-w-7xl flex-col px-5 py-5 sm:px-8 lg:px-10">
          <header className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-16 items-center justify-center overflow-hidden rounded-md border border-lime-300/30 bg-black">
                <img className="h-8 w-auto" src="/logo/logo.png" alt="Shadow SDK" />
              </div>
              <div>
                <p className="text-sm font-semibold text-lime-200">Shadow SDK</p>
                <p className="text-xs text-stone-400">Devnet private intents</p>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <span className="hidden rounded-md border border-stone-800 bg-stone-900/70 px-3 py-2 text-sm text-stone-300 sm:inline-flex">
                Devnet
              </span>
              <button
                className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-900 px-4 py-3 text-sm font-semibold text-stone-100 transition hover:border-lime-300/50 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
                type="button"
                onClick={openWalletPicker}
              >
                <Wallet aria-hidden="true" />
                Connect wallet
              </button>
            </div>
          </header>

          <div className="grid flex-1 items-center gap-10 py-16 lg:grid-cols-[1.05fr_0.95fr]">
            <div className="max-w-3xl">
              <p className="mb-4 inline-flex rounded-md border border-lime-300/30 bg-lime-300/10 px-3 py-1 text-sm font-medium text-lime-200">
                Private Solana intent execution
              </p>
              <h1 className="text-5xl font-semibold leading-tight tracking-normal text-stone-50 sm:text-6xl lg:text-7xl">
                Commit privately. Execute when the route is ready.
              </h1>
              <p className="mt-6 max-w-2xl text-lg leading-8 text-stone-300">
                Shadow keeps intent details private while devnet records the commitment hash and execution status.
              </p>
              <div className="mt-8 flex flex-col gap-3 sm:flex-row">
                <button
                  className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md bg-lime-300 px-5 py-3 font-semibold text-stone-950 transition hover:bg-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
                  type="button"
                  onClick={openWalletPicker}
                >
                  <Wallet aria-hidden="true" />
                  Get started
                </button>
                <a
                  className="inline-flex min-h-11 items-center justify-center rounded-md border border-stone-700 bg-stone-900/70 px-5 py-3 font-semibold text-stone-100 transition hover:border-stone-500 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
                  href="https://github.com/Vijaykv5/shadow-sdk#readme"
                  rel="noreferrer"
                  target="_blank"
                >
                  Docs
                </a>
              </div>
            </div>

            <div className="rounded-md border border-stone-800 bg-stone-950/60 p-5 shadow-2xl shadow-black/30">
              <div className="mb-5 flex items-center justify-between border-b border-stone-800 pb-4">
                <div>
                  <p className="text-sm text-stone-400">Current program</p>
                  <p className="mt-1 font-mono text-sm text-stone-100">{shortAddress(STEALTH_VAULT_PROGRAM_ID.toBase58())}</p>
                </div>
                <RadioTower className="text-lime-200" aria-hidden="true" />
              </div>
              <div className="grid gap-3">
                {[
                  ["Network", "Solana devnet"],
                  ["Relayer", relayerUrl],
                  ["Executor", shortAddress(DEVNET_EPHEMERAL_AUTHORITY)]
                ].map(([label, value]) => (
                  <div className="flex items-center justify-between rounded-md border border-stone-800 bg-stone-900/70 px-4 py-3" key={label}>
                    <span className="text-sm text-stone-400">{label}</span>
                    <span className="font-mono text-sm text-stone-100">{value}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className="min-h-screen bg-[#0f1110] px-5 py-5 text-stone-50 sm:px-8 lg:px-10">
      <ToastRegion toasts={toasts} />
      <div className="mx-auto max-w-6xl">
        <header className="mb-8 flex flex-col gap-4 border-b border-stone-800 pb-6 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <div className="mb-3 flex items-center gap-3">
              <div className="flex h-10 w-16 items-center justify-center overflow-hidden rounded-md border border-lime-300/30 bg-black">
                <img className="h-8 w-auto" src="/logo/logo.png" alt="Shadow SDK" />
              </div>
              <p className="text-sm font-semibold text-lime-200">Shadow SDK Console</p>
            </div>
            <h1 className="mt-2 text-3xl font-semibold tracking-normal text-stone-50 sm:text-4xl">Vault setup</h1>
          </div>
          <button
            className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-900 px-4 py-3 font-semibold text-stone-100 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
            type="button"
            onClick={openWalletPicker}
          >
            <Wallet aria-hidden="true" />
            {shortAddress(walletAddress)}
          </button>
        </header>

        {txMessage ? (
          <section
            className={`mb-5 flex items-center gap-2 rounded-md border px-4 py-3 text-sm ${
              txState === "error"
                ? "border-red-400/30 bg-red-400/10 text-red-100"
                : "border-stone-800 bg-stone-900 text-stone-100"
            }`}
            role={txState === "error" ? "alert" : "status"}
          >
            {txState === "sending" ? <Loader2 className="spin" aria-hidden="true" /> : txState === "error" ? <AlertCircle aria-hidden="true" /> : <Check aria-hidden="true" />}
            {txMessage}
          </section>
        ) : null}

        <section className="grid gap-6 lg:grid-cols-[0.8fr_1.2fr]">
          <aside className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
            <p className="text-sm font-semibold text-lime-200">Step 1</p>
            <h2 className="mt-2 text-2xl font-semibold">Create or refresh your vault</h2>
            <p className="mt-3 text-sm leading-6 text-stone-400">
              The vault binds your wallet to the relayer executor that will submit private intent hashes on devnet.
            </p>
          </aside>

          <section className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
            <div className="grid gap-4 md:grid-cols-2">
              <LabeledValue label="Owner wallet" value={owner} onChange={setOwner} error={owner && !ownerValid ? "Invalid Solana public key" : ""} />
              <LabeledValue label="Ephemeral authority" value={ephemeralAuthority} onChange={setEphemeralAuthority} />
              <LabeledValue label="Relayer URL" value={relayerUrl} onChange={setRelayerUrl} type="url" />
              <LabeledValue label="Executor keypair" value={executorKeypair} onChange={setExecutorKeypair} />
            </div>

            <div className="mt-5 flex flex-wrap gap-3">
              <button
                className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md bg-lime-300 px-5 py-3 font-semibold text-stone-950 transition hover:bg-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950 disabled:opacity-50"
                type="button"
                disabled={!ownerValid || !validatePubkey(ephemeralAuthority) || txState === "sending"}
                onClick={createVaultOnchain}
              >
                {txState === "sending" ? <Loader2 className="spin" aria-hidden="true" /> : <Wallet aria-hidden="true" />}
                Create on-chain
              </button>
              <button
                className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-900 px-5 py-3 font-semibold text-stone-100 transition hover:border-stone-500 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
                type="button"
                onClick={refreshVault}
              >
                <RefreshCw aria-hidden="true" />
                Refresh vault
              </button>
            </div>

            <div className="mt-6 rounded-md border border-stone-800 bg-stone-900/70 p-4">
              <h3 className="font-semibold">Vault account</h3>
              <dl className="mt-4 grid gap-3 text-sm">
                {[
                  ["PDA", vaultPda || "Unset"],
                  ["Owner", vaultAccount?.owner ?? "Not fetched"],
                  ["Ephemeral", vaultAccount?.ephemeralAuthority ?? "Not fetched"]
                ].map(([label, value]) => (
                  <div className="grid gap-1 sm:grid-cols-[140px_1fr]" key={label}>
                    <dt className="text-stone-500">{label}</dt>
                    <dd className="break-all font-mono text-stone-100">{value}</dd>
                  </div>
                ))}
              </dl>
            </div>
          </section>
        </section>

        {vaultAccount ? (
          <div ref={nextStepsRef} className="mt-8 grid gap-6 scroll-mt-6">
            <section className="grid gap-6 lg:grid-cols-[0.8fr_1.2fr]">
              <aside className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
                <p className="text-sm font-semibold text-lime-200">Step 2</p>
                <h2 className="mt-2 text-2xl font-semibold">Compose an intent</h2>
                <p className="mt-3 text-sm leading-6 text-stone-400">
                  Choose the intent shape, generate its hash, then submit that hash to devnet.
                </p>
              </aside>

              <section className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
                <div className="grid grid-cols-1 overflow-hidden rounded-md border border-stone-800 sm:grid-cols-3">
                  {(["mock_execution", "system_transfer", "perps_order"] as const).map((kind) => (
                    <button
                      className={`min-h-11 px-4 py-3 text-sm font-semibold transition focus-visible:ring-2 focus-visible:ring-lime-200 ${
                        composer.kind === kind
                          ? "bg-lime-300 text-stone-950"
                          : "bg-stone-900 text-stone-300 hover:bg-stone-800"
                      }`}
                      type="button"
                      key={kind}
                      onClick={() =>
                        setComposer((value) => ({
                          ...value,
                          kind,
                          transferTo:
                            kind === "system_transfer" ? value.transferTo || owner || walletAddress : value.transferTo
                        }))
                      }
                    >
                      {kind.replace("_", " ")}
                    </button>
                  ))}
                </div>
                <div className="mt-5">
                  <ComposerFields composer={composer} setComposer={setComposer} />
                </div>
                <div className="mt-4 flex items-center justify-between gap-3 rounded-md border border-stone-800 bg-stone-900/60 px-4 py-3">
                  <p className="text-sm text-stone-400">
                    Reusing a nonce creates the same intent PDA and will fail.
                  </p>
                  <button
                    className="inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-950 px-4 py-2 text-sm font-semibold text-stone-100 transition hover:border-stone-500 focus-visible:ring-2 focus-visible:ring-lime-200"
                    type="button"
                    onClick={assignFreshNonce}
                  >
                    <RefreshCw aria-hidden="true" />
                    New nonce
                  </button>
                </div>
                <div className="mt-5 flex flex-wrap gap-3">
                  <button
                    className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-900 px-5 py-3 font-semibold text-stone-100 transition hover:border-stone-500 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950"
                    type="button"
                    onClick={generatePayload}
                  >
                    <RefreshCw aria-hidden="true" />
                    Generate hash
                  </button>
                  <button
                    className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md bg-lime-300 px-5 py-3 font-semibold text-stone-950 transition hover:bg-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950 disabled:opacity-50"
                    type="button"
                    disabled={txState === "sending"}
                    onClick={submitIntentOnchain}
                  >
                    {txState === "sending" ? <Loader2 className="spin" aria-hidden="true" /> : <Wallet aria-hidden="true" />}
                    Submit hash on-chain
                  </button>
                </div>
              </section>
            </section>

            <section className="grid gap-6 lg:grid-cols-[0.8fr_1.2fr]">
              <aside className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
                <p className="text-sm font-semibold text-lime-200">Step 3</p>
                <h2 className="mt-2 text-2xl font-semibold">Review the commitment</h2>
                <p className="mt-3 text-sm leading-6 text-stone-400">
                  The payload stays off-chain. Devnet only receives the hash shown here.
                </p>
              </aside>

              <section className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
                <pre className="max-h-72 overflow-auto rounded-md border border-stone-800 bg-stone-900/80 p-4 font-mono text-sm leading-6 text-stone-200">
                  {payloadText || formatPayload(buildPayload(composer))}
                </pre>
                <div className="mt-4 flex items-center justify-between gap-3 rounded-md border border-stone-800 bg-stone-900/70 px-4 py-3">
                  <span className="min-w-0 break-all font-mono text-sm text-stone-200">
                    {payloadHash || "Generate or submit to calculate the hash"}
                  </span>
                  <button
                    aria-label="Copy payload hash"
                    className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-stone-700 bg-stone-950 text-stone-200 focus-visible:ring-2 focus-visible:ring-lime-200 disabled:opacity-50"
                    type="button"
                    disabled={!payloadHash}
                    onClick={() => copyText("hash", payloadHash)}
                  >
                    {copied === "hash" ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
                  </button>
                </div>
                <AccountReadout
                  title="Intent account"
                  rows={[
                    ["PDA", intentPda || "Unset"],
                    ["Status", intentAccount?.status ?? "Not fetched"],
                    ["Hash", intentAccount?.payloadHash ?? "Not fetched"],
                    ["Executor", intentAccount?.executor ?? "Not fetched"]
                  ]}
                />
              </section>
            </section>

            <section className="grid gap-6 lg:grid-cols-[0.8fr_1.2fr]">
              <aside className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
                <p className="text-sm font-semibold text-lime-200">Step 4</p>
                <h2 className="mt-2 text-2xl font-semibold">Queue and execute</h2>
                <p className="mt-3 text-sm leading-6 text-stone-400">
                  Queue the intent, then let the relayer verify it against the on-chain hash.
                </p>
              </aside>

              <section className="rounded-md border border-stone-800 bg-stone-950/60 p-5">
                <div className="flex flex-wrap gap-3">
                  <button
                    className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md bg-lime-300 px-5 py-3 font-semibold text-stone-950 transition hover:bg-lime-200 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950 disabled:opacity-50"
                    type="button"
                    disabled={!ownerValid || relayerState === "sending"}
                    onClick={queueInRelayer}
                    aria-busy={relayerState === "sending"}
                  >
                    {relayerState === "sending" ? <Loader2 className="spin" aria-hidden="true" /> : <FileJson aria-hidden="true" />}
                    Queue intent
                  </button>
                  <button
                    className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-900 px-5 py-3 font-semibold text-stone-100 transition hover:border-stone-500 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950 disabled:opacity-50"
                    type="button"
                    disabled={!persistedIntent || persistedIntent.status === "executed" || relayerState === "sending"}
                    onClick={executePersistedIntent}
                    aria-busy={relayerState === "sending"}
                  >
                    <ArrowRight aria-hidden="true" />
                    Execute
                  </button>
                  <button
                    className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-stone-700 bg-stone-900 px-5 py-3 font-semibold text-stone-100 transition hover:border-stone-500 focus-visible:ring-2 focus-visible:ring-lime-200 focus-visible:ring-offset-2 focus-visible:ring-offset-stone-950 disabled:opacity-50"
                    type="button"
                    disabled={!persistedIntent || relayerState === "sending"}
                    onClick={refreshPersistedIntent}
                  >
                    <RefreshCw aria-hidden="true" />
                    Check status
                  </button>
                </div>

                {relayerMessage ? (
                  <section
                    className={`mt-5 flex items-center gap-2 rounded-md border px-4 py-3 text-sm ${
                      relayerState === "error"
                        ? "border-red-400/30 bg-red-400/10 text-red-100"
                        : "border-stone-800 bg-stone-900 text-stone-100"
                    }`}
                    role={relayerState === "error" ? "alert" : "status"}
                  >
                    {relayerState === "sending" ? <Loader2 className="spin" aria-hidden="true" /> : relayerState === "error" ? <AlertCircle aria-hidden="true" /> : <Check aria-hidden="true" />}
                    {relayerMessage}
                  </section>
                ) : null}

                {persistedIntent ? (
                  <RelayerQueueCard
                    item={persistedIntent}
                    onCopy={(label, text) => copyText(label, text)}
                    copied={copied}
                  />
                ) : (
                  <div className="mt-5 rounded-md border border-dashed border-stone-800 bg-stone-900/40 p-5 text-sm text-stone-400">
                    No queued intent yet.
                  </div>
                )}
              </section>
            </section>
          </div>
        ) : (
          <section className="mt-8 rounded-md border border-dashed border-stone-800 bg-stone-950/40 p-6 text-sm text-stone-400">
            Create or refresh your vault to unlock payload, hash, and relayer execution steps.
          </section>
        )}
      </div>
    </main>
  );
}

function LabeledValue({
  label,
  value,
  onChange,
  error,
  type = "text"
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  error?: string;
  type?: string;
}) {
  const id = label.toLowerCase().replaceAll(" ", "-");
  return (
    <label className="grid gap-2 text-sm" htmlFor={id}>
      <span className="font-medium text-stone-300">{label}</span>
      <input
        className="min-h-11 rounded-md border border-stone-800 bg-stone-900 px-3 font-mono text-sm text-stone-100 outline-none transition placeholder:text-stone-600 focus-visible:border-lime-300 focus-visible:ring-2 focus-visible:ring-lime-200/50"
        id={id}
        type={type}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {error ? <em className="not-italic text-red-200">{error}</em> : null}
    </label>
  );
}

function ToastRegion({ toasts }: { toasts: Toast[] }) {
  if (toasts.length === 0) return null;

  return (
    <div
      aria-live="polite"
      className="fixed bottom-4 right-4 z-[60] grid w-[min(360px,calc(100vw-32px))] gap-3"
    >
      {toasts.map((toast) => (
        <article
          className={`rounded-md border p-4 shadow-2xl shadow-black/40 ${
            toast.tone === "error"
              ? "border-red-400/30 bg-red-950/90 text-red-50"
              : toast.tone === "success"
                ? "border-lime-300/30 bg-stone-950/95 text-stone-50"
                : "border-stone-700 bg-stone-950/95 text-stone-50"
          }`}
          key={toast.id}
        >
          <div className="flex items-start gap-3">
            <span
              className={`mt-1 h-2.5 w-2.5 shrink-0 rounded-full ${
                toast.tone === "error"
                  ? "bg-red-300"
                  : toast.tone === "success"
                    ? "bg-lime-300"
                    : "bg-stone-400"
              }`}
              aria-hidden="true"
            />
            <div className="min-w-0">
              <strong className="block text-sm">{toast.title}</strong>
              {toast.message ? (
                <p className="mt-1 break-words text-sm text-stone-300">{toast.message}</p>
              ) : null}
            </div>
          </div>
        </article>
      ))}
    </div>
  );
}

function buildPayload(composer: ComposerState): IntentPayload {
  const base = {
    nonce: Number(composer.nonce),
    expires_at: composer.expiresAt ? Math.floor(new Date(composer.expiresAt).getTime() / 1000) : null
  };

  if (composer.kind === "system_transfer") {
    return {
      ...base,
      kind: "system_transfer",
      payload: {
        to: composer.transferTo.trim(),
        lamports: Number(composer.transferLamports)
      }
    };
  }

  if (composer.kind === "perps_order") {
    return {
      ...base,
      kind: "perps_order",
      payload: {
        venue: composer.perpsVenue,
        market: composer.perpsMarket,
        side: composer.perpsSide,
        size_base_lots: Number(composer.perpsSizeLots),
        limit_price: Number(composer.perpsLimitPrice),
        max_slippage_bps: Number(composer.perpsSlippageBps),
        reduce_only: composer.perpsReduceOnly,
        client_order_id: composer.perpsClientOrderId
      },
      route: buildExecutionRoute(composer)
    };
  }

  return {
    ...base,
    kind: "mock_execution",
    payload: {
      message: composer.mockMessage
    }
  };
}

function getComposerValidationError(composer: ComposerState) {
  if (composer.kind !== "system_transfer") return "";

  if (!validatePubkey(composer.transferTo)) {
    return "Enter a valid recipient wallet before sending a system transfer.";
  }

  if (!Number.isFinite(composer.transferLamports) || composer.transferLamports <= 0) {
    return "Lamports must be greater than zero.";
  }

  if (composer.transferLamports < MIN_NEW_ACCOUNT_TRANSFER_LAMPORTS) {
    return `Use at least ${MIN_NEW_ACCOUNT_TRANSFER_LAMPORTS.toLocaleString()} lamports for the demo. Tiny transfers fail when the recipient account is new on devnet.`;
  }

  return "";
}

function buildExecutionRoute(composer: ComposerState): ExecutionRoute {
  if (composer.routeKind === "jito_bundle") {
    return {
      kind: "jito_bundle",
      tip_lamports: Number(composer.tipLamports)
    };
  }

  if (composer.routeKind === "magicblock_er") {
    return {
      kind: "magicblock_er",
      validator: composer.magicBlockValidator,
      commit_frequency_ms: Number(composer.commitFrequencyMs)
    };
  }

  if (composer.routeKind === "magicblock_per") {
    return {
      kind: "magicblock_per",
      validator: "devnet_tee",
      commit_frequency_ms: Number(composer.commitFrequencyMs)
    };
  }

  return {
    kind: "mock_private_bundle",
    tip_lamports: Number(composer.tipLamports)
  };
}

function formatSubmitError(error: unknown) {
  const message = error instanceof Error ? error.message : "Failed to submit intent";
  if (message.includes("intent already exists") || message.includes("already in use")) {
    return "This nonce already has an on-chain intent. Click New nonce, then generate and submit again.";
  }

  return message;
}

function ComposerFields({
  composer,
  setComposer
}: {
  composer: ComposerState;
  setComposer: React.Dispatch<React.SetStateAction<ComposerState>>;
}) {
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <NumberField label="Nonce" value={composer.nonce} onChange={(nonce) => setComposer((value) => ({ ...value, nonce }))} />
      <Field
        label="Expires at"
        type="datetime-local"
        value={composer.expiresAt}
        onChange={(expiresAt) => setComposer((value) => ({ ...value, expiresAt }))}
      />
      {composer.kind === "mock_execution" ? (
        <Field
          label="Message"
          value={composer.mockMessage}
          onChange={(mockMessage) => setComposer((value) => ({ ...value, mockMessage }))}
        />
      ) : null}
      {composer.kind === "system_transfer" ? (
        <>
          <Field
            label="Recipient"
            value={composer.transferTo}
            onChange={(transferTo) => setComposer((value) => ({ ...value, transferTo }))}
            placeholder="Recipient public key"
          />
          <NumberField
            label="Lamports"
            value={composer.transferLamports}
            onChange={(transferLamports) => setComposer((value) => ({ ...value, transferLamports }))}
            hint={`Use ${MIN_NEW_ACCOUNT_TRANSFER_LAMPORTS.toLocaleString()}+ lamports for new devnet recipients.`}
          />
        </>
      ) : null}
      {composer.kind === "perps_order" ? (
        <>
          <SelectField
            label="Venue"
            value={composer.perpsVenue}
            options={["mock", "drift"]}
            onChange={(perpsVenue) => setComposer((value) => ({ ...value, perpsVenue }))}
          />
          <Field
            label="Market"
            value={composer.perpsMarket}
            onChange={(perpsMarket) => setComposer((value) => ({ ...value, perpsMarket }))}
          />
          <SelectField
            label="Side"
            value={composer.perpsSide}
            options={["long", "short"]}
            onChange={(perpsSide) => setComposer((value) => ({ ...value, perpsSide }))}
          />
          <NumberField
            label="Size lots"
            value={composer.perpsSizeLots}
            onChange={(perpsSizeLots) => setComposer((value) => ({ ...value, perpsSizeLots }))}
          />
          <NumberField
            label="Limit price"
            value={composer.perpsLimitPrice}
            onChange={(perpsLimitPrice) => setComposer((value) => ({ ...value, perpsLimitPrice }))}
          />
          <NumberField
            label="Slippage bps"
            value={composer.perpsSlippageBps}
            onChange={(perpsSlippageBps) => setComposer((value) => ({ ...value, perpsSlippageBps }))}
          />
          <SelectField
            label="Route"
            value={composer.routeKind}
            options={ROUTE_OPTIONS}
            onChange={(routeKind) => setComposer((value) => ({ ...value, routeKind }))}
          />
          {composer.routeKind === "mock_private_bundle" || composer.routeKind === "jito_bundle" ? (
            <NumberField
              label="Tip lamports"
              value={composer.tipLamports}
              onChange={(tipLamports) => setComposer((value) => ({ ...value, tipLamports }))}
            />
          ) : null}
          {composer.routeKind === "magicblock_er" ? (
            <SelectField
              label="MagicBlock validator"
              value={composer.magicBlockValidator}
              options={MAGICBLOCK_VALIDATOR_OPTIONS}
              onChange={(magicBlockValidator) =>
                setComposer((value) => ({ ...value, magicBlockValidator }))
              }
            />
          ) : null}
          {composer.routeKind === "magicblock_per" ? (
            <Field
              label="MagicBlock TEE"
              value="devnet_tee"
              onChange={() => undefined}
              readOnly
            />
          ) : null}
          {composer.routeKind === "magicblock_er" || composer.routeKind === "magicblock_per" ? (
            <NumberField
              label="Commit ms"
              value={composer.commitFrequencyMs}
              onChange={(commitFrequencyMs) =>
                setComposer((value) => ({ ...value, commitFrequencyMs }))
              }
            />
          ) : null}
          <Field
            label="Client order id"
            value={composer.perpsClientOrderId}
            onChange={(perpsClientOrderId) => setComposer((value) => ({ ...value, perpsClientOrderId }))}
          />
          <label className="checkbox-field">
            <input
              checked={composer.perpsReduceOnly}
              type="checkbox"
              onChange={(event) =>
                setComposer((value) => ({ ...value, perpsReduceOnly: event.target.checked }))
              }
            />
            Reduce only
          </label>
        </>
      ) : null}
    </div>
  );
}

function PanelHeader({ icon, title, action }: { icon: React.ReactNode; title: string; action: string }) {
  return (
    <div className="panel-header">
      <div>
        <span className="panel-icon" aria-hidden="true">
          {icon}
        </span>
        <h2>{title}</h2>
      </div>
      <span className="badge">{action}</span>
    </div>
  );
}

function Stat({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <article className="stat">
      <span aria-hidden="true">{icon}</span>
      <div>
        <p>{label}</p>
        <strong>{value}</strong>
      </div>
    </article>
  );
}

function ReadinessItem({
  status,
  label,
  value
}: {
  status: "ready" | "waiting";
  label: string;
  value: string;
}) {
  return (
    <article className="readiness-item">
      <StatusDot status={status === "ready" ? "executed" : "pending"} />
      <div>
        <p>{label}</p>
        <strong>{value}</strong>
      </div>
    </article>
  );
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  error,
  readOnly = false,
  type = "text"
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  error?: string;
  readOnly?: boolean;
  type?: string;
}) {
  const id = label.toLowerCase().replaceAll(" ", "-");
  return (
    <label className="grid gap-2 text-sm" htmlFor={id}>
      <span className="font-medium text-stone-300">{label}</span>
      <input
        className="min-h-11 rounded-md border border-stone-800 bg-stone-900 px-3 text-sm text-stone-100 outline-none transition placeholder:text-stone-600 focus-visible:border-lime-300 focus-visible:ring-2 focus-visible:ring-lime-200/50"
        id={id}
        type={type}
        value={value}
        placeholder={placeholder}
        readOnly={readOnly}
        onChange={(event) => onChange(event.target.value)}
      />
      {error ? <em className="not-italic text-red-200">{error}</em> : null}
    </label>
  );
}

function NumberField({
  label,
  value,
  onChange,
  hint
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  hint?: string;
}) {
  const id = label.toLowerCase().replaceAll(" ", "-");
  return (
    <label className="grid gap-2 text-sm" htmlFor={id}>
      <span className="font-medium text-stone-300">{label}</span>
      <input
        className="min-h-11 rounded-md border border-stone-800 bg-stone-900 px-3 text-sm text-stone-100 outline-none transition placeholder:text-stone-600 focus-visible:border-lime-300 focus-visible:ring-2 focus-visible:ring-lime-200/50"
        id={id}
        min={0}
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      {hint ? <span className="text-xs text-stone-500">{hint}</span> : null}
    </label>
  );
}

function SelectField<T extends string>({
  label,
  value,
  options,
  onChange
}: {
  label: string;
  value: T;
  options: T[];
  onChange: (value: T) => void;
}) {
  const id = label.toLowerCase().replaceAll(" ", "-");
  return (
    <label className="grid gap-2 text-sm" htmlFor={id}>
      <span className="font-medium text-stone-300">{label}</span>
      <select
        className="min-h-11 rounded-md border border-stone-800 bg-stone-900 px-3 text-sm text-stone-100 outline-none transition focus-visible:border-lime-300 focus-visible:ring-2 focus-visible:ring-lime-200/50"
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value as T)}
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </label>
  );
}

function CommandBlock({
  title,
  command,
  onCopy,
  copied
}: {
  title: string;
  command: string;
  onCopy: (command: string) => void;
  copied: boolean;
}) {
  return (
    <div className="mt-5 overflow-hidden rounded-md border border-stone-800 bg-stone-950/60">
      <div className="flex items-center justify-between border-b border-stone-800 px-4 py-3">
        <strong>{title}</strong>
        <button
          aria-label={`Copy ${title}`}
          className="inline-flex h-10 w-10 items-center justify-center rounded-md border border-stone-700 bg-stone-900 text-stone-200 focus-visible:ring-2 focus-visible:ring-lime-200"
          type="button"
          onClick={() => onCopy(command)}
        >
          {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
        </button>
      </div>
      <pre className="overflow-auto p-4 font-mono text-sm leading-6 text-stone-300">{command}</pre>
    </div>
  );
}

function RelayerQueueCard({
  item,
  copied,
  onCopy
}: {
  item: RelayerQueuedIntent;
  copied: string | null;
  onCopy: (label: string, text: string) => void;
}) {
  const rows: [string, string][] = [
    ["ID", item.id],
    ["Status", item.status],
    ["Owner", item.owner],
    ["Nonce", item.nonce.toString()],
    ["Hash", item.payload_hash],
    ["Updated", new Date(item.updated_at * 1000).toLocaleString()]
  ];

  if (item.error) {
    rows.push(["Error", item.error]);
  }

  return (
    <div className="mt-5 rounded-md border border-stone-800 bg-stone-900/60 p-4">
      <div className="mb-4 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <StatusDot status={item.status} />
          <strong>Queued intent</strong>
          <span className="rounded-md border border-stone-700 px-2 py-1 text-xs text-stone-300">{item.status}</span>
        </div>
        <button
          aria-label="Copy queued intent ID"
          className="inline-flex h-10 w-10 items-center justify-center rounded-md border border-stone-700 bg-stone-950 text-stone-200 focus-visible:ring-2 focus-visible:ring-lime-200"
          type="button"
          onClick={() => onCopy("relayer-queue-id", item.id)}
        >
          {copied === "relayer-queue-id" ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
        </button>
      </div>
      <AccountReadout title="Relayer record" rows={rows} />
    </div>
  );
}

function StatusDot({ status }: { status: QueueItem["status"] | RelayerQueuedIntent["status"] }) {
  const color =
    status === "executed"
      ? "bg-lime-300"
      : status === "failed"
        ? "bg-red-300"
        : status === "executing"
          ? "bg-amber-300"
          : "bg-stone-500";

  return <span className={`h-2.5 w-2.5 rounded-full ${color}`} aria-label={status} />;
}

function AccountReadout({ title, rows }: { title: string; rows: [string, string][] }) {
  return (
    <div className="mt-5 rounded-md border border-stone-800 bg-stone-900/70 p-4">
      <strong>{title}</strong>
      {rows.map(([label, value]) => (
        <div className="mt-3 grid gap-1 text-sm sm:grid-cols-[140px_1fr]" key={label}>
          <span className="text-stone-500">{label}</span>
          <code className="break-all font-mono text-stone-100">{value}</code>
        </div>
      ))}
    </div>
  );
}
