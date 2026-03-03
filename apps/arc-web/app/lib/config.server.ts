import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { parse } from "smol-toml";

interface AuthConfig {
  provider: "github" | "insecure_disabled";
  allowed_usernames: string[];
}

interface ApiConfig {
  base_url: string;
  authentication_strategy: "jwt" | "insecure_disabled";
}

export interface AppConfig {
  auth: AuthConfig;
  api: ApiConfig;
}

const AUTH_DEFAULTS: AuthConfig = {
  provider: "github",
  allowed_usernames: [],
};

const API_DEFAULTS: ApiConfig = {
  base_url: "http://localhost:3000",
  authentication_strategy: "jwt",
};

let cached: AppConfig | null = null;

export function getAppConfig(): AppConfig {
  if (cached) return cached;

  const configPath = join(homedir(), ".arc", "arc.toml");

  let raw: Record<string, unknown> = {};
  try {
    raw = parse(readFileSync(configPath, "utf-8")) as Record<string, unknown>;
  } catch {
    // File doesn't exist or is unreadable — use defaults
  }

  const rawAuth = (raw.auth ?? {}) as Partial<AuthConfig>;
  const rawApi = (raw.api ?? {}) as Partial<ApiConfig>;

  cached = {
    auth: { ...AUTH_DEFAULTS, ...rawAuth },
    api: { ...API_DEFAULTS, ...rawApi },
  };

  return cached;
}

/** Reset cached config (for testing). */
export function resetAppConfigCache(): void {
  cached = null;
}
