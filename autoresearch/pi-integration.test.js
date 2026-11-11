import { expect, test } from "bun:test";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { getPackageDir, ModelRuntime, SettingsManager } from "@earendil-works/pi-coding-agent";

test("Pi public settings and model APIs work in an isolated server directory", async () => {
  const root = await mkdtemp(join(tmpdir(), "syntaxis-pi-sdk-"));
  const agentDir = join(root, "agent");
  try {
    await mkdir(agentDir);
    const settings = SettingsManager.create(root, agentDir, { projectTrusted: false });
    settings.setCacheWarmingMode("streaming");
    await settings.flush();
    expect(settings.drainErrors()).toEqual([]);
    expect(JSON.parse(await readFile(join(agentDir, "settings.json"), "utf8")).cacheWarming).toBe(
      "streaming",
    );
    expect(SettingsManager.create(root, agentDir).getCacheWarmingMode()).toBe("streaming");

    const runtime = await ModelRuntime.create({
      authPath: join(agentDir, "auth.json"),
      modelsPath: join(agentDir, "models.json"),
      modelsStorePath: join(agentDir, "models-store.json"),
      allowModelNetwork: false,
    });
    expect(runtime.getModels().length).toBeGreaterThan(0);
    expect(runtime.getModel("openai", "gpt-6.1-sol")).toBeDefined();
    expect(await runtime.listCredentials()).toEqual([]);
    await runtime.settleCredentialWrites();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test.each(["node", process.execPath])(
  "Pi %s RPC answers the server adapter's startup commands without credentials",
  async (executable) => {
    const root = await mkdtemp(join(tmpdir(), "syntaxis-pi-rpc-"));
    const manifest = JSON.parse(await readFile(join(getPackageDir(), "package.json"), "utf8"));
    const child = spawn(
      executable,
      [
        join(getPackageDir(), manifest.bin.pi),
        "--mode",
        "rpc",
        "--no-session",
        "--no-extensions",
        "--no-skills",
        "--no-prompt-templates",
      ],
      {
        cwd: root,
        env: {
          PATH: process.env.PATH,
          PI_CODING_AGENT_DIR: join(root, "agent"),
          PI_SKIP_VERSION_CHECK: "1",
          PI_TELEMETRY: "0",
          NO_COLOR: "1",
        },
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
    let stderr = "";
    child.stderr.on("data", (chunk) => {
      stderr = (stderr + chunk).slice(-8192);
    });
    const closed = new Promise((resolve) => child.once("close", resolve));
    const lines = createInterface({ input: child.stdout });
    const commands = [
      "get_state",
      "get_messages",
      "get_fork_messages",
      "get_commands",
      "get_available_models",
      "get_session_stats",
    ];
    let timer;
    try {
      const replies = await new Promise((resolve, reject) => {
        const received = new Map();
        timer = setTimeout(() => reject(new Error(`Pi RPC timed out: ${stderr}`)), 20_000);
        child.once("error", reject);
        child.once("exit", (code) => reject(new Error(`Pi RPC exited (${code}): ${stderr}`)));
        lines.on("line", (line) => {
          try {
            const record = JSON.parse(line);
            if (record.type !== "response" || !commands.includes(record.id)) return;
            if (!record.success) throw new Error(JSON.stringify(record));
            received.set(record.id, record.data);
            if (received.size === commands.length) resolve(received);
          } catch (error) {
            reject(error);
          }
        });
        for (const type of commands) child.stdin.write(`${JSON.stringify({ id: type, type })}\n`);
      });
      expect(replies.get("get_state").isStreaming).toBe(false);
      expect(replies.get("get_messages").messages).toEqual([]);
      expect(replies.get("get_fork_messages").messages).toEqual([]);
      expect(Array.isArray(replies.get("get_commands").commands)).toBe(true);
      expect(replies.get("get_available_models").models).toEqual([]);
      expect(replies.get("get_session_stats").totalMessages).toBe(0);
    } finally {
      clearTimeout(timer);
      lines.close();
      child.kill("SIGKILL");
      await closed;
      await rm(root, { recursive: true, force: true });
    }
  },
  30_000,
);
