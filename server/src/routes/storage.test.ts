import { afterEach, beforeEach, describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { app } from "../app.js";
import { createUser, resetDb } from "../test-utils.js";

const SECRET = "internal-s3cret";
let savedWebdavUrl: string | undefined;
let savedAuthSecret: string | undefined;
let savedSavesDir: string | undefined;
let tmpSaves: string;

beforeEach(() => {
  resetDb();
  savedWebdavUrl = process.env.WEBDAV_PUBLIC_URL;
  savedAuthSecret = process.env.WEBDAV_AUTH_SECRET;
  savedSavesDir = process.env.SAVES_DIR;
  process.env.WEBDAV_AUTH_SECRET = SECRET;
  tmpSaves = fs.mkdtempSync(path.join(os.tmpdir(), "spool-saves-"));
  process.env.SAVES_DIR = tmpSaves;
});

afterEach(() => {
  const restore = (k: string, v: string | undefined) =>
    v === undefined ? delete process.env[k] : (process.env[k] = v);
  restore("WEBDAV_PUBLIC_URL", savedWebdavUrl);
  restore("WEBDAV_AUTH_SECRET", savedAuthSecret);
  restore("SAVES_DIR", savedSavesDir);
  fs.rmSync(tmpSaves, { recursive: true, force: true });
});

function getStorage(apiKey?: string) {
  const headers: Record<string, string> = {};
  if (apiKey !== undefined) headers["Authorization"] = `Bearer ${apiKey}`;
  return app.request("/storage", { headers });
}

function webdavAuth(body: unknown, secret?: string) {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (secret !== undefined) headers["X-Internal-Secret"] = secret;
  return app.request("/internal/webdav-auth", {
    method: "POST",
    headers,
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}

describe("GET /storage", () => {
  it("rejects an unauthenticated request", async () => {
    process.env.WEBDAV_PUBLIC_URL = "https://host:47634";
    expect((await getStorage()).status).toBe(401);
  });

  it("404s when WEBDAV_PUBLIC_URL is not set (storage disabled)", async () => {
    delete process.env.WEBDAV_PUBLIC_URL;
    const { apiKey } = createUser("alice");
    expect((await getStorage(apiKey)).status).toBe(404);
  });

  it("returns connection details and provisions the account dir when enabled", async () => {
    process.env.WEBDAV_PUBLIC_URL = "https://host:47634";
    const { id, apiKey } = createUser("alice");

    const res = await getStorage(apiKey);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toMatchObject({
      webdav_url: "https://host:47634",
      username: "alice",
      password: apiKey,
      base_path: "ludusavi-backup",
      provider: "other",
    });
    expect(fs.existsSync(path.join(tmpSaves, id))).toBe(true);
  });
});

describe("POST /internal/webdav-auth", () => {
  it("forbids without the internal secret", async () => {
    const { apiKey } = createUser("alice");
    expect((await webdavAuth({ user: "alice", pass: apiKey })).status).toBe(403);
  });

  it("forbids with the wrong internal secret", async () => {
    const { apiKey } = createUser("alice");
    expect((await webdavAuth({ user: "alice", pass: apiKey }, "nope")).status).toBe(403);
  });

  it("rejects an unknown api key", async () => {
    expect((await webdavAuth({ user: "alice", pass: "not-a-key" }, SECRET)).status).toBe(401);
  });

  it("rejects when the username doesn't match the api key's account", async () => {
    const { apiKey } = createUser("alice");
    expect((await webdavAuth({ user: "bob", pass: apiKey }, SECRET)).status).toBe(401);
  });

  it("returns a local backend jailed to the account's dir for valid creds", async () => {
    const { id, apiKey } = createUser("alice");
    const res = await webdavAuth({ user: "alice", pass: apiKey }, SECRET);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ type: "local", _root: `${tmpSaves}/${id}` });
  });
});
