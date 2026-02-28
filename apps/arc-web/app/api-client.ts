import { importPKCS8, SignJWT } from "jose";

const ARC_API_BASE_URL = process.env.ARC_API_BASE_URL;
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
  if (!ARC_API_BASE_URL) {
    throw new Error("ARC_API_BASE_URL environment variable is not set");
  }

  const token = await signToken();
  const headers = new Headers(init?.headers);
  headers.set("Authorization", `Bearer ${token}`);

  return fetch(`${ARC_API_BASE_URL}${path}`, {
    ...init,
    headers,
  });
}
