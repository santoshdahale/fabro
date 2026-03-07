import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { randomBytes } from "node:crypto";
import { redirect } from "react-router";
import { parse, stringify } from "smol-toml";
import { ARC_CONFIG_PATH, reloadAppConfig } from "../lib/config.server";
import { mergeEnv } from "../lib/merge-env";
import type { Route } from "./+types/setup-callback";

const ENV_PATH = resolve(import.meta.dirname, "../../../../.env");

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const code = url.searchParams.get("code");
  if (!code) {
    return { error: "Missing code parameter" };
  }

  const response = await fetch(
    `https://api.github.com/app-manifests/${code}/conversions`,
    { method: "POST", headers: { Accept: "application/vnd.github+json" } }
  );

  if (!response.ok) {
    const body = await response.text();
    return { error: `GitHub API error: ${response.status} ${body}` };
  }

  const data = (await response.json()) as {
    id: number;
    slug: string;
    client_id: string;
    client_secret: string;
    webhook_secret: string;
    pem: string;
  };

  const sessionSecret = randomBytes(32).toString("hex");

  // Write non-secrets to TOML config
  let tomlConfig: Record<string, unknown> = {};
  try {
    tomlConfig = parse(await readFile(ARC_CONFIG_PATH, "utf-8")) as Record<string, unknown>;
  } catch {
    // file doesn't exist yet
  }
  tomlConfig.git = {
    ...((tomlConfig.git as Record<string, unknown>) ?? {}),
    provider: "github",
    app_id: String(data.id),
    client_id: data.client_id,
    slug: data.slug,
  };
  await mkdir(dirname(ARC_CONFIG_PATH), { recursive: true });
  await writeFile(ARC_CONFIG_PATH, stringify(tomlConfig), "utf-8");
  reloadAppConfig();

  // Write secrets to .env (merge to avoid duplicates on re-run)
  let existing = "";
  try {
    existing = await readFile(ENV_PATH, "utf-8");
  } catch {
    // file doesn't exist yet
  }

  const envContent = mergeEnv(
    existing,
    new Map([
      ["SESSION_SECRET", sessionSecret],
      ["GITHUB_APP_CLIENT_SECRET", data.client_secret],
      ["GITHUB_APP_WEBHOOK_SECRET", data.webhook_secret],
      ["GITHUB_APP_PRIVATE_KEY", Buffer.from(data.pem).toString("base64")],
    ]),
  );
  await writeFile(ENV_PATH, envContent, "utf-8");

  process.env.SESSION_SECRET = sessionSecret;
  process.env.GITHUB_APP_CLIENT_SECRET = data.client_secret;
  process.env.GITHUB_APP_WEBHOOK_SECRET = data.webhook_secret;
  process.env.GITHUB_APP_PRIVATE_KEY = Buffer.from(data.pem).toString("base64");

  throw redirect("/auth/login");
}

export default function SetupCallback({ loaderData }: Route.ComponentProps) {
  const { error } = loaderData;

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-atmosphere px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 flex justify-center">
          <img src="/logo.svg" alt="Arc" className="h-12 w-12" draggable={false} />
        </div>
        <div className="rounded-xl border border-line bg-panel/80 p-8 shadow-lg backdrop-blur-sm">
          <div className="mx-auto mb-4 flex h-10 w-10 items-center justify-center rounded-full bg-coral/10">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" className="text-coral">
              <path d="M7 7L13 13M13 7L7 13" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </div>
          <h1 className="text-center text-lg font-semibold text-fg">
            Setup failed
          </h1>
          <p className="mt-2 text-center text-sm text-fg-3">{error}</p>
          <a
            href="/setup"
            className="mt-6 flex w-full items-center justify-center rounded-lg border border-line-strong px-4 py-2.5 text-sm font-medium text-fg-2 transition-colors hover:bg-overlay-strong"
          >
            Try again
          </a>
        </div>
      </div>
    </div>
  );
}
