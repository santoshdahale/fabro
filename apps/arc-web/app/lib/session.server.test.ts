import { describe, test, expect, beforeEach, mock } from "bun:test";

// --- Mocks (must be set up before importing module under test) ---

let testAuthConfig = { provider: "github" as string, allowed_usernames: [] as string[] };

mock.module("./config.server", () => ({
  getAppConfig: () => ({ web: { auth: testAuthConfig } }),
  reloadAppConfig: () => {},
  ARC_CONFIG_PATH: "/tmp/test.toml",
}));

let sessionData: Record<string, unknown> = {};

mock.module("./session-storage.server", () => ({
  createSqliteSessionStorage: () => ({
    getSession: async () => ({
      get: (key: string) => sessionData[key],
    }),
    commitSession: async () => "",
    destroySession: async () => "",
  }),
}));

process.env.SESSION_SECRET = "test-secret";

const { getUser } = await import("./session.server");

// --- Tests ---

describe("getUser", () => {
  beforeEach(() => {
    sessionData = {};
    testAuthConfig = { provider: "github", allowed_usernames: [] };
  });

  describe("tailscale provider", () => {
    beforeEach(() => {
      testAuthConfig = { provider: "tailscale", allowed_usernames: ["user@example.com"] };
    });

    test("returns user from headers when login is in allowed_usernames", async () => {
      const request = new Request("http://localhost", {
        headers: {
          "Tailscale-User-Login": "user@example.com",
          "Tailscale-User-Name": "Test User",
          "Tailscale-User-Profile-Pic": "https://example.com/pic.jpg",
        },
      });

      const user = await getUser(request);

      expect(user).toEqual({
        userUrl: "tailscale:user@example.com",
        login: "user@example.com",
        name: "Test User",
        email: "user@example.com",
        avatarUrl: "https://example.com/pic.jpg",
      });
    });

    test("returns null when Tailscale-User-Login header is missing", async () => {
      const request = new Request("http://localhost");

      const user = await getUser(request);

      expect(user).toBeNull();
    });

    test("returns null when login is not in allowed_usernames", async () => {
      const request = new Request("http://localhost", {
        headers: {
          "Tailscale-User-Login": "stranger@example.com",
          "Tailscale-User-Name": "Stranger",
        },
      });

      const user = await getUser(request);

      expect(user).toBeNull();
    });
  });

  describe("github provider", () => {
    beforeEach(() => {
      testAuthConfig = { provider: "github", allowed_usernames: [] };
    });

    test("returns user from session", async () => {
      sessionData = {
        userUrl: "https://github.com/octocat",
        login: "octocat",
        name: "Octocat",
        email: "octocat@github.com",
        avatarUrl: "https://github.com/octocat.png",
      };
      const request = new Request("http://localhost");

      const user = await getUser(request);

      expect(user).toEqual({
        userUrl: "https://github.com/octocat",
        login: "octocat",
        name: "Octocat",
        email: "octocat@github.com",
        avatarUrl: "https://github.com/octocat.png",
      });
    });

    test("returns null when session is empty", async () => {
      sessionData = {};
      const request = new Request("http://localhost");

      const user = await getUser(request);

      expect(user).toBeNull();
    });
  });
});
