import { redirect } from "react-router";
import { getAppConfig } from "./config.server";
import { createSqliteSessionStorage } from "./session-storage.server";

interface SessionData {
  userUrl: string;
  githubId: number;
  githubNodeId: string;
  login: string;
  name: string;
  email: string;
  avatarUrl: string;
  accessToken: string;
}

function getSessionStorage() {
  const secret = process.env.SESSION_SECRET;
  if (!secret) {
    throw new Error("SESSION_SECRET is not set");
  }
  return createSqliteSessionStorage(secret);
}

export async function getSession(request: Request) {
  const storage = getSessionStorage();
  return storage.getSession(request.headers.get("Cookie"));
}

export async function commitSession(session: Awaited<ReturnType<typeof getSession>>) {
  const storage = getSessionStorage();
  return storage.commitSession(session);
}

export async function destroySession(session: Awaited<ReturnType<typeof getSession>>) {
  const storage = getSessionStorage();
  return storage.destroySession(session);
}

export async function getUser(request: Request) {
  const { provider, allowed_usernames } = getAppConfig().web.auth;

  if (provider === "tailscale") {
    const login = request.headers.get("Tailscale-User-Login");
    if (!login || !allowed_usernames.includes(login)) return null;
    return {
      userUrl: `tailscale:${login}`,
      login,
      name: request.headers.get("Tailscale-User-Name") ?? login,
      email: login,
      avatarUrl: request.headers.get("Tailscale-User-Profile-Pic") ?? "",
    };
  }

  const session = await getSession(request);
  const login = session.get("login");
  if (!login) return null;
  return {
    userUrl: session.get("userUrl") ?? "",
    login,
    name: session.get("name") ?? login,
    email: session.get("email") ?? "",
    avatarUrl: session.get("avatarUrl") ?? "",
  };
}

export async function requireUser(request: Request) {
  const user = await getUser(request);
  if (!user) throw redirect("/auth/login");
  return user;
}
