import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { NextRequest, NextResponse } from "next/server";

export async function POST(request: NextRequest) {
  const body = (await request.json()) as {
    payload?: string;
    nonce?: number;
    kind?: string;
  };

  if (!body.payload || typeof body.payload !== "string") {
    return NextResponse.json({ error: "payload is required" }, { status: 400 });
  }

  const repoRoot = path.resolve(process.cwd(), "../..");
  const pendingDir = path.join(repoRoot, "payloads", "pending");
  await mkdir(pendingDir, { recursive: true });

  const safeKind = String(body.kind ?? "intent").replace(/[^a-z0-9_-]/gi, "-");
  const nonce = Number.isFinite(body.nonce) ? body.nonce : "unknown";
  const fileName = `${Date.now()}-${safeKind}-${nonce}.json`;
  const filePath = path.join(pendingDir, fileName);

  await writeFile(filePath, body.payload, "utf8");

  return NextResponse.json({
    path: path.relative(repoRoot, filePath)
  });
}
