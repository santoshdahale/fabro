import { importPKCS8, SignJWT } from "jose";
import { getAppConfig } from "./lib/config.server";

const ARC_JWT_PRIVATE_KEY = process.env.ARC_JWT_PRIVATE_KEY;

let cachedKey: CryptoKey | null = null;

async function getSigningKey(): Promise<CryptoKey> {
  if (cachedKey) return cachedKey;
  if (!ARC_JWT_PRIVATE_KEY) {
    throw new Error("ARC_JWT_PRIVATE_KEY environment variable is not set");
  }
  cachedKey = await importPKCS8(ARC_JWT_PRIVATE_KEY, "EdDSA");
  return cachedKey;
}

async function signToken(): Promise<string> {
  const key = await getSigningKey();
  return new SignJWT({ iss: "arc-web" })
    .setProtectedHeader({ alg: "EdDSA" })
    .setIssuedAt()
    .setExpirationTime("30s")
    .sign(key);
}

/**
 * Fetch wrapper that signs requests with a JWT for service-to-service auth.
 */
export async function apiFetch(
  path: string,
  init?: RequestInit
): Promise<Response> {
  const { base_url } = getAppConfig().api;

  const headers = new Headers(init?.headers);
  if (ARC_JWT_PRIVATE_KEY) {
    const token = await signToken();
    headers.set("Authorization", `Bearer ${token}`);
  }

  return fetch(`${base_url}${path}`, {
    ...init,
    headers,
  });
}

/**
 * Typed JSON fetch helper. Calls apiFetch and parses the JSON response.
 */
export async function apiJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await apiFetch(path, init);
  if (!res.ok) throw new Response(null, { status: res.status });
  return res.json() as Promise<T>;
}
