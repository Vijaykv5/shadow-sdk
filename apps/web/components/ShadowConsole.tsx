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
  Shield,
  Terminal,
  Wallet,
  XCircle
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
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
  executeIntentWithRelayer,
  fetchExecutionIntentAccount,
  fetchVaultAccount,
  formatPayload,
  hashIntentPayload,
  hashPayloadBytes,
  IntentKind,
  IntentPayload,
  prepareTransaction,
  QueueItem,
  shortAddress,
  STEALTH_VAULT_PROGRAM_ID,
  validatePubkey,
  type ExecutionIntentAccount,
  type Cluster,
  type ExecutionRoute,
  type VaultAccount
} from "@/lib/shadow";
import {
  getAvailableWallets,
  getWalletById,
  type WalletProviderOption
} from "@/lib/wallet";

const QUEUE_KEY = "shadow-sdk.intent-queue";
const DEVNET_DEPLOY_WALLET = "2eDJJZydDTV4HQmbtX6YwhrdfCW7XU3zms9538HGqkuB";

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
};

const initialComposer: ComposerState = {
  kind: "mock_execution",
  nonce: 1,
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
  tipLamports: 5000
};

export function ShadowConsole() {
  const [cluster, setCluster] = useState<Cluster>("localnet");
  const [walletAddress, setWalletAddress] = useState("");
  const [owner, setOwner] = useState("");
  const [ephemeralAuthority, setEphemeralAuthority] = useState("");
  const [executorKeypair, setExecutorKeypair] = useState("~/.config/solana/ephemeral.json");
  const [payloadDir, setPayloadDir] = useState("payloads");
  const [relayerUrl, setRelayerUrl] = useState(DEFAULT_RELAYER_URL);
  const [composer, setComposer] = useState<ComposerState>(initialComposer);
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [payloadText, setPayloadText] = useState("");
  const [payloadHash, setPayloadHash] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [walletState, setWalletState] = useState<"idle" | "connecting" | "connected" | "error">(
    "idle"
  );
  const [selectedWalletId, setSelectedWalletId] = useState("");
  const [walletPickerOpen, setWalletPickerOpen] = useState(false);
  const [walletProviders, setWalletProviders] = useState<WalletProviderOption[]>([]);
  const [walletError, setWalletError] = useState("");
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

  const rpcUrl = clusterRpcUrl(cluster);
  const connection = useMemo(() => new Connection(rpcUrl, "confirmed"), [rpcUrl]);

  useEffect(() => {
    const saved = window.localStorage.getItem(QUEUE_KEY);
    if (saved) {
      setQueue(JSON.parse(saved) as QueueItem[]);
    }
    setWalletProviders(getAvailableWallets());
  }, []);

  useEffect(() => {
    window.localStorage.setItem(QUEUE_KEY, JSON.stringify(queue));
  }, [queue]);

  useEffect(() => {
    if (!walletPickerOpen) return;

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setWalletPickerOpen(false);
      }
    }

    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [walletPickerOpen]);

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
    setWalletProviders(getAvailableWallets());
    setWalletError("");
    setWalletPickerOpen(true);
  }

  async function connectWallet(providerId: string) {
    setWalletState("connecting");
    setWalletError("");
    const wallet = getWalletById(providerId);
    if (!wallet) {
      setWalletState("error");
      setWalletError("That wallet provider was not found. Refresh the page and try again.");
      return;
    }

    try {
      const result = await wallet.connect();
      const publicKey = result?.publicKey ?? wallet.publicKey;
      if (!publicKey) {
        throw new Error("Wallet connected, but did not expose a public key.");
      }

      const address = publicKey.toBase58();
      setWalletAddress(address);
      setOwner(address);
      setEphemeralAuthority((current) => current || address);
      setSelectedWalletId(providerId);
      setWalletPickerOpen(false);
      setWalletState("connected");
    } catch (error) {
      setWalletState("error");
      setWalletError(error instanceof Error ? error.message : "Wallet connection was rejected.");
    }
  }

  async function sendWalletTransaction(transaction: Transaction, signerAddress: string) {
    const wallet = selectedWalletId ? getWalletById(selectedWalletId) : null;
    if (!wallet) throw new Error("Browser wallet is not connected");
    const prepared = await prepareTransaction(connection, transaction, signerAddress);

    if (wallet.signAndSendTransaction) {
      return (await wallet.signAndSendTransaction(prepared)).signature;
    }

    if (!wallet.signTransaction) {
      throw new Error("Wallet does not support transaction signing");
    }

    const signed = await wallet.signTransaction(prepared);
    return connection.sendRawTransaction(signed.serialize());
  }

  async function createVaultOnchain() {
    if (!walletAddress || !owner || !ephemeralAuthority) return;
    if (walletAddress !== owner) {
      setTxState("error");
      setTxMessage("Connected wallet must match the owner wallet to create this vault.");
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
      setTxMessage(`Vault transaction sent: ${signature}`);
      await refreshVault();
    } catch (error) {
      setTxState("error");
      setTxMessage(error instanceof Error ? error.message : "Failed to create vault");
    }
  }

  async function submitIntentOnchain() {
    if (!walletAddress || !owner || !payloadHash) return;
    if (walletAddress !== ephemeralAuthority) {
      setTxState("error");
      setTxMessage("Connected wallet must match the ephemeral authority to submit this intent.");
      return;
    }

    setTxState("sending");
    setTxMessage("Submitting intent...");
    try {
      const signature = await sendWalletTransaction(
        createSubmitIntentTransaction({
          owner,
          ephemeralAuthority,
          nonce: composer.nonce,
          payloadHash
        }),
        walletAddress
      );
      setTxState("success");
      setTxMessage(`Intent transaction sent: ${signature}`);
      await refreshIntent();
    } catch (error) {
      setTxState("error");
      setTxMessage(error instanceof Error ? error.message : "Failed to submit intent");
    }
  }

  async function executeViaRelayer() {
    if (!ownerValid) {
      setRelayerState("error");
      setRelayerMessage("Set a valid owner wallet before calling the relayer.");
      return;
    }

    setRelayerState("sending");
    setRelayerMessage("Sending private payload to relayer...");
    setRelayerResult(null);

    try {
      const payload = buildPayload(composer);
      const hash = payloadHash || (await hashIntentPayload(payload));
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
      setRelayerMessage(`Relayer executed intent: ${result.signature}`);
      await refreshIntent();
    } catch (error) {
      setRelayerState("error");
      setRelayerMessage(error instanceof Error ? error.message : "Relayer execution failed");
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
      return;
    }
    setSavedPayloadPath(result.path ?? "");
    setTxState("success");
    setTxMessage(`Payload saved to ${result.path}`);
  }

  async function generatePayload() {
    const payload = buildPayload(composer);
    const text = formatPayload(payload);
    setPayloadText(text);
    setPayloadHash(await hashIntentPayload(payload));
  }

  async function addToQueue() {
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
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">Shadow SDK Console</p>
          <h1>Private intent execution</h1>
        </div>
        <div className="topbar-actions">
          <select
            aria-label="Cluster"
            className="select"
            value={cluster}
            onChange={(event) => setCluster(event.target.value as Cluster)}
          >
            <option value="localnet">Localnet</option>
            <option value="devnet">Devnet</option>
          </select>
          <button className="button button-primary" type="button" onClick={openWalletPicker}>
            {walletState === "connecting" ? <Loader2 className="spin" /> : <Wallet />}
            {walletAddress ? shortAddress(walletAddress) : "Connect wallet"}
          </button>
        </div>
      </header>

      {walletState === "error" ? (
        <section className="notice notice-error" role="alert">
          <AlertCircle />
          {walletError || "Wallet not found, not selected, or connection was rejected."}
        </section>
      ) : null}

      {walletPickerOpen ? (
        <WalletPicker
          providers={walletProviders}
          connecting={walletState === "connecting"}
          selectedWalletId={selectedWalletId}
          onRefresh={() => setWalletProviders(getAvailableWallets())}
          onConnect={connectWallet}
          onClose={() => setWalletPickerOpen(false)}
        />
      ) : null}

      {txMessage ? (
        <section className={txState === "error" ? "notice notice-error" : "notice"} role="status">
          {txState === "sending" ? <Loader2 className="spin" /> : txState === "error" ? <AlertCircle /> : <Check />}
          {txMessage}
        </section>
      ) : null}

      <section className="stats-grid" aria-label="Project state">
        <Stat icon={<Shield />} label="Vault PDA" value={vaultPda ? shortAddress(vaultPda) : "Unset"} />
        <Stat icon={<KeyRound />} label="Intent PDA" value={intentPda ? shortAddress(intentPda) : "Unset"} />
        <Stat icon={<Activity />} label="Queue" value={`${queue.length} intents`} />
        <Stat icon={<RadioTower />} label="RPC" value={clusterRpcUrl(cluster)} />
      </section>

      <section className="panel deploy-panel">
        <PanelHeader icon={<RadioTower />} title="Devnet Deploy Readiness" action="Next" />
        <div className="readiness-grid">
          <ReadinessItem
            status={cluster === "devnet" ? "ready" : "waiting"}
            label="Web cluster"
            value={cluster === "devnet" ? "Devnet selected" : "Switch app cluster to Devnet before wallet testing"}
          />
          <ReadinessItem
            status="ready"
            label="Program id"
            value={STEALTH_VAULT_PROGRAM_ID.toBase58()}
          />
          <ReadinessItem
            status="waiting"
            label="Deploy wallet"
            value={`${shortAddress(DEVNET_DEPLOY_WALLET)} needs devnet SOL before deploy`}
          />
          <ReadinessItem
            status={walletAddress ? "ready" : "waiting"}
            label="Test wallet"
            value={walletAddress ? shortAddress(walletAddress) : "Connect wallet after deployment"}
          />
        </div>
        <CommandBlock
          title="Deploy Devnet Program"
          command={deployCommand}
          onCopy={(text) => copyText("deploy", text)}
          copied={copied === "deploy"}
        />
      </section>

      <div className="dashboard-grid">
        <section className="panel">
          <PanelHeader icon={<Shield />} title="Vault Setup" action="Step 1" />
          <div className="field-grid">
            <Field
              label="Owner wallet"
              value={owner}
              onChange={setOwner}
              placeholder="Owner public key"
              error={owner && !ownerValid ? "Invalid Solana public key" : ""}
            />
            <Field
              label="Ephemeral authority"
              value={ephemeralAuthority}
              onChange={setEphemeralAuthority}
              placeholder="Temporary executor public key"
            />
            <Field
              label="Executor keypair"
              value={executorKeypair}
              onChange={setExecutorKeypair}
              placeholder="~/.config/solana/ephemeral.json"
            />
            <Field
              label="Relayer payload directory"
              value={payloadDir}
              onChange={setPayloadDir}
              placeholder="payloads"
            />
            <Field
              label="Relayer URL"
              value={relayerUrl}
              onChange={setRelayerUrl}
              placeholder="http://127.0.0.1:8787"
              type="url"
            />
          </div>
          <CommandBlock
            title="Create Vault"
            command={[
              "cargo run -p shadow-cli -- create-vault \\",
              `  --ephemeral-authority ${ephemeralAuthority || "<EPHEMERAL_AUTHORITY>"}`
            ].join("\n")}
            onCopy={(text) => copyText("create-vault", text)}
            copied={copied === "create-vault"}
          />
          <div className="button-row">
            <button
              className="button button-primary"
              type="button"
              disabled={!walletAddress || !ownerValid || !validatePubkey(ephemeralAuthority) || txState === "sending"}
              onClick={createVaultOnchain}
            >
              <Wallet />
              Create on-chain
            </button>
            <button className="button" type="button" disabled={!vaultPda} onClick={refreshVault}>
              <RefreshCw />
              Refresh vault
            </button>
          </div>
          <AccountReadout
            title="Vault account"
            rows={[
              ["PDA", vaultPda || "Unset"],
              ["Owner", vaultAccount?.owner ?? "Not fetched"],
              ["Ephemeral", vaultAccount?.ephemeralAuthority ?? "Not fetched"]
            ]}
          />
        </section>

        <section className="panel">
          <PanelHeader icon={<FileJson />} title="Intent Composer" action="Step 2" />
          <div className="segmented" role="tablist" aria-label="Intent type">
            {(["mock_execution", "system_transfer", "perps_order"] as const).map((kind) => (
              <button
                className={composer.kind === kind ? "segment active" : "segment"}
                type="button"
                key={kind}
                onClick={() => setComposer((value) => ({ ...value, kind }))}
              >
                {kind.replace("_", " ")}
              </button>
            ))}
          </div>
          <ComposerFields composer={composer} setComposer={setComposer} />
          <div className="button-row">
            <button className="button" type="button" onClick={generatePayload}>
              <RefreshCw />
              Generate hash
            </button>
            <button className="button button-primary" type="button" onClick={addToQueue}>
              <ArrowRight />
              Add to queue
            </button>
            <button className="button" type="button" onClick={savePayloadToPending}>
              <FileJson />
              Save pending
            </button>
          </div>
          {savedPayloadPath ? <p className="inline-note">Saved: {savedPayloadPath}</p> : null}
        </section>
      </div>

      <div className="dashboard-grid lower">
        <section className="panel panel-tall">
          <PanelHeader icon={<Clipboard />} title="Payload and Hash" action="Step 3" />
          <pre className="payload-preview">{payloadText || formatPayload(buildPayload(composer))}</pre>
          <div className="hash-row">
            <span>{payloadHash || "Generate a hash to submit on-chain"}</span>
            <button
              aria-label="Copy payload hash"
              className="icon-button"
              type="button"
              disabled={!payloadHash}
              onClick={() => copyText("hash", payloadHash)}
            >
              {copied === "hash" ? <Check /> : <Copy />}
            </button>
          </div>
          <CommandBlock
            title="Submit Intent"
            command={submitIntentCommand}
            onCopy={(text) => copyText("submit", text)}
            copied={copied === "submit"}
          />
          <div className="button-row">
            <button
              className="button button-primary"
              type="button"
              disabled={!walletAddress || !payloadHash || txState === "sending"}
              onClick={submitIntentOnchain}
            >
              <Wallet />
              Submit on-chain
            </button>
            <button className="button" type="button" disabled={!intentPda} onClick={refreshIntent}>
              <RefreshCw />
              Refresh intent
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

        <section className="panel panel-tall">
          <PanelHeader icon={<Terminal />} title="Relayer API" action="Step 4" />
          <CommandBlock
            title="Start Relayer API"
            command={runRelayerCommand}
            onCopy={(text) => copyText("relayer", text)}
            copied={copied === "relayer"}
          />
          <div className="button-row">
            <button
              className="button button-primary"
              type="button"
              disabled={!ownerValid || !intentPda || relayerState === "sending"}
              onClick={executeViaRelayer}
              aria-busy={relayerState === "sending"}
            >
              {relayerState === "sending" ? <Loader2 className="spin" /> : <RadioTower />}
              Execute via relayer
            </button>
            <button
              className="button"
              type="button"
              disabled={relayerState === "sending"}
              onClick={() => {
                setRelayerMessage("");
                setRelayerState("idle");
                setRelayerResult(null);
              }}
            >
              <RefreshCw />
              Reset
            </button>
          </div>
          {relayerMessage ? (
            <section
              className={relayerState === "error" ? "notice notice-error compact-notice" : "notice compact-notice"}
              role={relayerState === "error" ? "alert" : "status"}
            >
              {relayerState === "sending" ? (
                <Loader2 className="spin" />
              ) : relayerState === "error" ? (
                <AlertCircle />
              ) : (
                <Check />
              )}
              {relayerMessage}
            </section>
          ) : null}
          {relayerResult ? (
            <AccountReadout
              title="Relayer result"
              rows={[
                ["Signature", relayerResult.signature],
                ["Intent", relayerResult.intent],
                ["Vault", relayerResult.vault],
                ["Hash", relayerResult.payloadHash]
              ]}
            />
          ) : null}
          <div className="queue-list">
            {queue.length === 0 ? (
              <div className="empty-state">
                <FileJson />
                <p>No local queue items yet.</p>
              </div>
            ) : (
              queue.map((item) => (
                <article className="queue-item" key={item.id}>
                  <div>
                    <div className="queue-title">
                      <StatusDot status={item.status} />
                      <strong>{item.kind}</strong>
                      <span>nonce {item.nonce}</span>
                    </div>
                    <p>{shortAddress(item.hash)}</p>
                  </div>
                  <div className="queue-actions">
                    <button type="button" className="icon-button" onClick={() => copyText(item.id, item.payload)}>
                      {copied === item.id ? <Check /> : <Copy />}
                    </button>
                    <button type="button" className="icon-button" onClick={() => updateQueue(item.id, "executed")}>
                      <Check />
                    </button>
                    <button
                      type="button"
                      className="icon-button"
                      onClick={() => updateQueue(item.id, "failed", "Marked failed in console")}
                    >
                      <XCircle />
                    </button>
                  </div>
                </article>
              ))
            )}
          </div>
        </section>
      </div>

      <section className="panel">
        <PanelHeader icon={<Terminal />} title="Relayer Config" action="Optional" />
        <pre className="config-preview">{configText}</pre>
      </section>
    </main>
  );
}

function WalletPicker({
  providers,
  connecting,
  selectedWalletId,
  onRefresh,
  onConnect,
  onClose
}: {
  providers: WalletProviderOption[];
  connecting: boolean;
  selectedWalletId: string;
  onRefresh: () => void;
  onConnect: (providerId: string) => void;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        aria-labelledby="wallet-picker-title"
        aria-modal="true"
        className="wallet-modal"
        role="dialog"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="wallet-modal-header">
          <div>
            <p className="eyebrow">Wallet provider</p>
            <h2 id="wallet-picker-title">Choose a wallet</h2>
          </div>
          <div className="wallet-header-actions">
            <button aria-label="Refresh wallet providers" className="icon-button" type="button" onClick={onRefresh}>
              <RefreshCw />
            </button>
            <button aria-label="Close wallet picker" className="icon-button" type="button" onClick={onClose}>
              <XCircle />
            </button>
          </div>
        </div>

        {providers.length === 0 ? (
          <div className="wallet-empty">
            <Wallet />
            <strong>No Solana wallet detected</strong>
            <p>Install Phantom, Solflare, Backpack, or another injected Solana wallet, then refresh this page.</p>
          </div>
        ) : (
          <div className="wallet-list">
            {providers.map((provider) => (
              <button
                className="wallet-option"
                disabled={connecting}
                key={provider.id}
                type="button"
                onClick={() => onConnect(provider.id)}
              >
                <span className="wallet-mark" aria-hidden="true">
                  <Wallet />
                </span>
                <span>
                  <strong>{provider.name}</strong>
                  <small>{provider.id === selectedWalletId ? "Connected provider" : "Detected in browser"}</small>
                </span>
                {connecting ? <Loader2 className="spin" /> : <ArrowRight />}
              </button>
            ))}
          </div>
        )}
      </section>
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
        to: composer.transferTo || "So11111111111111111111111111111111111111112",
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
      route: {
        kind: composer.routeKind,
        tip_lamports: Number(composer.tipLamports)
      } as ExecutionRoute
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

function ComposerFields({
  composer,
  setComposer
}: {
  composer: ComposerState;
  setComposer: React.Dispatch<React.SetStateAction<ComposerState>>;
}) {
  return (
    <div className="field-grid compact">
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
            options={["mock_private_bundle", "jito_bundle"]}
            onChange={(routeKind) => setComposer((value) => ({ ...value, routeKind }))}
          />
          <NumberField
            label="Tip lamports"
            value={composer.tipLamports}
            onChange={(tipLamports) => setComposer((value) => ({ ...value, tipLamports }))}
          />
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
  type = "text"
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  error?: string;
  type?: string;
}) {
  const id = label.toLowerCase().replaceAll(" ", "-");
  return (
    <label className="field" htmlFor={id}>
      <span>{label}</span>
      <input
        id={id}
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      {error ? <em>{error}</em> : null}
    </label>
  );
}

function NumberField({
  label,
  value,
  onChange
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  const id = label.toLowerCase().replaceAll(" ", "-");
  return (
    <label className="field" htmlFor={id}>
      <span>{label}</span>
      <input
        id={id}
        min={0}
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
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
    <label className="field" htmlFor={id}>
      <span>{label}</span>
      <select id={id} value={value} onChange={(event) => onChange(event.target.value as T)}>
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
    <div className="command-block">
      <div>
        <strong>{title}</strong>
        <button aria-label={`Copy ${title}`} className="icon-button" type="button" onClick={() => onCopy(command)}>
          {copied ? <Check /> : <Copy />}
        </button>
      </div>
      <pre>{command}</pre>
    </div>
  );
}

function StatusDot({ status }: { status: QueueItem["status"] }) {
  return <span className={`status-dot ${status}`} aria-label={status} />;
}

function AccountReadout({ title, rows }: { title: string; rows: [string, string][] }) {
  return (
    <div className="account-readout">
      <strong>{title}</strong>
      {rows.map(([label, value]) => (
        <div key={label}>
          <span>{label}</span>
          <code>{value}</code>
        </div>
      ))}
    </div>
  );
}
