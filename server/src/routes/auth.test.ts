import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { app } from "../app.js";
import { createUser, resetDb } from "../test-utils.js";

const ADMIN_SECRET = "s3cret";
let savedSecret: string | undefined;

beforeEach(() => {
  resetDb();
  savedSecret = process.env.ADMIN_SECRET;
  process.env.ADMIN_SECRET = ADMIN_SECRET;
});

afterEach(() => {
  if (savedSecret === undefined) delete process.env.ADMIN_SECRET;
  else process.env.ADMIN_SECRET = savedSecret;
});

function register(body: unknown, secret?: string) {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (secret !== undefined) headers["X-Admin-Secret"] = secret;
  return app.request("/auth/register", {
    method: "POST",
    headers,
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}

describe("POST /auth/register", () => {
  it("forbids registration without the admin secret header", async () => {
    expect((await register({ username: "alice" })).status).toBe(403);
  });

  it("forbids registration with the wrong admin secret", async () => {
    expect((await register({ username: "alice" }, "wrong")).status).toBe(403);
  });

  it("forbids registration when no admin secret is configured", async () => {
    delete process.env.ADMIN_SECRET;
    expect((await register({ username: "alice" }, "")).status).toBe(403);
  });

  it("rejects a malformed JSON body", async () => {
    expect((await register("{ bad", ADMIN_SECRET)).status).toBe(400);
  });

  it("requires a username", async () => {
    expect((await register({}, ADMIN_SECRET)).status).toBe(400);
    expect((await register({ username: "  " }, ADMIN_SECRET)).status).toBe(400);
  });

  it("creates a user and returns an api key", async () => {
    const res = await register({ username: "alice" }, ADMIN_SECRET);
    expect(res.status).toBe(201);
    const body = await res.json();
    expect(typeof body.api_key).toBe("string");
    expect(body.api_key.length).toBeGreaterThan(0);
  });

  it("rejects a duplicate username", async () => {
    createUser("alice");
    expect((await register({ username: "alice" }, ADMIN_SECRET)).status).toBe(409);
  });
});
