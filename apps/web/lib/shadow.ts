import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction
} from "@solana/web3.js";
import { Buffer } from "buffer";

export const STEALTH_VAULT_PROGRAM_ID = new PublicKey(
  "3Nz8wUHewqpMuceSLnoeTMyPLaDt9kNzsVMWTCeVMD6M"
);

export type IntentKind = "mock_execution" | "system_transfer" | "perps_order";
export type IntentStatus = "draft" | "pending" | "executed" | "cancelled" | "failed";
export type Cluster = "devnet";

export type MockPayload = {
  message: string;
};

export type TransferPayload = {
  to: string;
  lamports: number;
};

export type PerpsPayload = {
  venue: "mock" | "drift";
  market: string;
  side: "long" | "short";
  size_base_lots: number;
  limit_price: number;
  max_slippage_bps: number;
  reduce_only: boolean;
  client_order_id: string;
};

export type ExecutionRoute =
  | { kind: "public_rpc" }
  | { kind: "mock_private_bundle"; tip_lamports: number }
  | { kind: "jito_bundle"; tip_lamports: number }
  | {
      kind: "magicblock_er";
      validator: MagicBlockValidator;
      commit_frequency_ms: number;
    }
  | {
      kind: "magicblock_per";
      validator: MagicBlockValidator;
      commit_frequency_ms: number;
    };

export type MagicBlockValidator =
  | "local_er"
  | "devnet_asia"
  | "devnet_eu"
  | "devnet_us"
  | "devnet_tee";

export const MAGICBLOCK_VALIDATORS: Record<MagicBlockValidator, string> = {
  local_er: "mAGicPQYBMvcYveUZA5F5UNNwyHvfYh5xkLS2Fr1mev",
  devnet_asia: "MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57",
  devnet_eu: "MEUGGrYPxKk17hCr7wpT6s8dtNokZj5U2L57vjYMS8e",
  devnet_us: "MUS3hc9TCw4cGC12vHNoYcCGzJG1txjgQLZWVoeNHNd",
  devnet_tee: "MTEWGuqxUpYZGFJQcp8tLN7x5v9BSeoFHYWQQ3n3xzo"
};

export type IntentPayload = {
  nonce: number;
  kind: IntentKind;
  payload: MockPayload | TransferPayload | PerpsPayload;
  route?: ExecutionRoute;
  expires_at: number | null;
};

export type QueueItem = {
  id: string;
  status: IntentStatus;
  kind: IntentKind;
  nonce: number;
  hash: string;
  payload: string;
  createdAt: string;
  error?: string;
};

export type VaultAccount = {
  owner: string;
  ephemeralAuthority: string;
  bump: number;
};

export type ExecutionIntentAccount = {
  vault: string;
  ephemeralAuthority: string;
  executor: string;
  nonce: number;
  payloadHash: string;
  status: IntentStatus;
  createdAt: number;
  cancelledAt: number;
  executedAt: number;
  bump: number;
};

export type RelayerExecuteResponse = {
  intent: string;
  vault: string;
  owner: string;
  executor: string;
  nonce: number;
  signature: string;
  payload_hash: string;
};

export type RelayerSubmitIntentResponse = {
  intent: string;
  vault: string;
  owner: string;
  ephemeral_authority: string;
  nonce: number;
  signature: string;
  payload_hash: string;
};

export type RelayerQueueStatus = "queued" | "executing" | "executed" | "failed";

export type RelayerQueuedIntent = {
  id: string;
  owner: string;
  nonce: number;
  status: RelayerQueueStatus;
  payload_hash: string;
  created_at: number;
  updated_at: number;
  error: string | null;
};

export const DEFAULT_RELAYER_URL =
  process.env.NEXT_PUBLIC_RELAYER_URL ?? "http://127.0.0.1:8787";

const DISCRIMINATORS = {
  initializeVault: Uint8Array.from([48, 191, 163, 44, 71, 129, 63, 164]),
  submitExecutionIntent: Uint8Array.from([144, 64, 85, 209, 247, 3, 129, 47]),
  vaultAccount: Uint8Array.from([211, 8, 232, 43, 2, 152, 117, 119]),
  executionIntentAccount: Uint8Array.from([220, 63, 72, 180, 147, 230, 49, 49])
};

export function deriveVaultPda(owner: string): string {
  const [vault] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("vault"), new PublicKey(owner).toBytes()],
    STEALTH_VAULT_PROGRAM_ID
  );
  return vault.toBase58();
}

export function deriveIntentPda(vault: string, nonce: number): string {
  const [intent] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("intent"), new PublicKey(vault).toBytes(), u64Bytes(nonce)],
    STEALTH_VAULT_PROGRAM_ID
  );
  return intent.toBase58();
}

export function createInitializeVaultTransaction(owner: string, ephemeralAuthority: string) {
  const ownerPubkey = new PublicKey(owner);
  const vault = new PublicKey(deriveVaultPda(owner));
  const data = concatBytes(DISCRIMINATORS.initializeVault, new PublicKey(ephemeralAuthority).toBytes());

  return new Transaction().add(
    new TransactionInstruction({
      programId: STEALTH_VAULT_PROGRAM_ID,
      keys: [
        { pubkey: ownerPubkey, isSigner: true, isWritable: true },
        { pubkey: vault, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
      ],
      data: Buffer.from(data)
    })
  );
}

export function createSubmitIntentTransaction({
  owner,
  ephemeralAuthority,
  nonce,
  payloadHash
}: {
  owner: string;
  ephemeralAuthority: string;
  nonce: number;
  payloadHash: string;
}) {
  const vault = deriveVaultPda(owner);
  const data = concatBytes(
    DISCRIMINATORS.submitExecutionIntent,
    u64Bytes(nonce),
    hexToBytes(payloadHash)
  );

  return new Transaction().add(
    new TransactionInstruction({
      programId: STEALTH_VAULT_PROGRAM_ID,
      keys: [
        { pubkey: new PublicKey(ephemeralAuthority), isSigner: true, isWritable: true },
        { pubkey: new PublicKey(vault), isSigner: false, isWritable: false },
        { pubkey: new PublicKey(deriveIntentPda(vault, nonce)), isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
      ],
      data: Buffer.from(data)
    })
  );
}

export async function prepareTransaction(
  connection: Connection,
  transaction: Transaction,
  feePayer: string
) {
  transaction.feePayer = new PublicKey(feePayer);
  transaction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  return transaction;
}

export async function fetchVaultAccount(
  connection: Connection,
  vault: string
): Promise<VaultAccount | null> {
  const account = await connection.getAccountInfo(new PublicKey(vault), "confirmed");
  if (
    !account ||
    !account.owner.equals(STEALTH_VAULT_PROGRAM_ID) ||
    account.data.length < 73 ||
    !bytesEqual(account.data.subarray(0, 8), DISCRIMINATORS.vaultAccount)
  ) {
    return null;
  }

  return {
    owner: new PublicKey(account.data.subarray(8, 40)).toBase58(),
    ephemeralAuthority: new PublicKey(account.data.subarray(40, 72)).toBase58(),
    bump: account.data[72] ?? 0
  };
}

export async function fetchExecutionIntentAccount(
  connection: Connection,
  intent: string
): Promise<ExecutionIntentAccount | null> {
  const account = await connection.getAccountInfo(new PublicKey(intent), "confirmed");
  if (
    !account ||
    !account.owner.equals(STEALTH_VAULT_PROGRAM_ID) ||
    account.data.length < 170 ||
    !bytesEqual(account.data.subarray(0, 8), DISCRIMINATORS.executionIntentAccount)
  ) {
    return null;
  }

  const data = account.data;
  return {
    vault: new PublicKey(data.subarray(8, 40)).toBase58(),
    ephemeralAuthority: new PublicKey(data.subarray(40, 72)).toBase58(),
    executor: new PublicKey(data.subarray(72, 104)).toBase58(),
    nonce: Number(readU64(data, 104)),
    payloadHash: bytesToHex(data.subarray(112, 144)),
    status: intentStatus(data[144] ?? 0),
    createdAt: Number(readI64(data, 145)),
    cancelledAt: Number(readI64(data, 153)),
    executedAt: Number(readI64(data, 161)),
    bump: data[169] ?? 0
  };
}

export async function hashPayloadBytes(payload: string): Promise<string> {
  const bytes = new TextEncoder().encode(payload);
  const data = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(data).set(bytes);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export async function hashIntentPayload(payload: IntentPayload): Promise<string> {
  return hashPayloadBytes(canonicalPayload(payload));
}

export function formatPayload(value: IntentPayload): string {
  return `${JSON.stringify(value, null, 2)}\n`;
}

export function canonicalPayload(value: IntentPayload): string {
  return stableStringify(value);
}

function stableStringify(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableStringify(item)).join(",")}]`;
  }

  if (value && typeof value === "object") {
    return `{${Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableStringify(item)}`)
      .join(",")}}`;
  }

  return JSON.stringify(value);
}

export async function executeIntentWithRelayer({
  relayerUrl,
  owner,
  payload
}: {
  relayerUrl: string;
  owner: string;
  payload: IntentPayload;
}): Promise<RelayerExecuteResponse> {
  return relayerRequest<RelayerExecuteResponse>(relayerUrl, "/execute-once", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ owner, payload })
  });
}

export async function submitIntentWithRelayer({
  relayerUrl,
  owner,
  nonce,
  payloadHash
}: {
  relayerUrl: string;
  owner: string;
  nonce: number;
  payloadHash: string;
}): Promise<RelayerSubmitIntentResponse> {
  return relayerRequest<RelayerSubmitIntentResponse>(relayerUrl, "/submit-intent", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ owner, nonce, payload_hash: payloadHash })
  });
}

export async function queueIntentWithRelayer({
  relayerUrl,
  owner,
  payload
}: {
  relayerUrl: string;
  owner: string;
  payload: IntentPayload;
}): Promise<RelayerQueuedIntent> {
  return relayerRequest<RelayerQueuedIntent>(relayerUrl, "/intents", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ owner, payload })
  });
}

export async function getRelayerIntent({
  relayerUrl,
  id
}: {
  relayerUrl: string;
  id: string;
}): Promise<RelayerQueuedIntent> {
  return relayerRequest<RelayerQueuedIntent>(relayerUrl, `/intents/${encodeURIComponent(id)}`);
}

export async function executeQueuedIntentWithRelayer({
  relayerUrl,
  id
}: {
  relayerUrl: string;
  id: string;
}): Promise<RelayerQueuedIntent> {
  return relayerRequest<RelayerQueuedIntent>(
    relayerUrl,
    `/intents/${encodeURIComponent(id)}/execute`,
    {
      method: "POST"
    }
  );
}

export function clusterRpcUrl(cluster: Cluster): string {
  return "https://api.devnet.solana.com";
}

export function shortAddress(address: string): string {
  if (!address) return "";
  return `${address.slice(0, 4)}...${address.slice(-4)}`;
}

export function validatePubkey(value: string): boolean {
  try {
    new PublicKey(value);
    return true;
  } catch {
    return false;
  }
}

function intentStatus(status: number): IntentStatus {
  if (status === 1) return "cancelled";
  if (status === 2) return "executed";
  return "pending";
}

function concatBytes(...parts: Uint8Array[]) {
  const output = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function bytesEqual(left: Uint8Array, right: Uint8Array) {
  return left.length === right.length && left.every((byte, index) => byte === right[index]);
}

function u64Bytes(value: number) {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), true);
  return bytes;
}

function readU64(bytes: Uint8Array, offset: number) {
  return new DataView(bytes.buffer, bytes.byteOffset + offset, 8).getBigUint64(0, true);
}

function readI64(bytes: Uint8Array, offset: number) {
  return new DataView(bytes.buffer, bytes.byteOffset + offset, 8).getBigInt64(0, true);
}

function hexToBytes(hex: string) {
  const normalized = hex.trim();
  if (normalized.length !== 64) {
    throw new Error("payload hash must be 64 hex characters");
  }
  return Uint8Array.from(normalized.match(/.{1,2}/g)?.map((byte) => parseInt(byte, 16)) ?? []);
}

function bytesToHex(bytes: Uint8Array) {
  return Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function relayerRequest<T>(
  relayerUrl: string,
  path: string,
  init?: RequestInit
): Promise<T> {
  const response = await fetch(`${relayerUrl.replace(/\/$/, "")}${path}`, init);
  const result = (await response.json()) as T & { error?: string };

  if (!response.ok) {
    throw new Error(result.error ?? "Relayer request failed");
  }

  return result;
}
