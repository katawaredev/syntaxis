import { afterEach, expect, test } from "bun:test";
import "./bridge-source.js";

const bridge = globalThis.SyntaxisBrowserAi;
const originalFetch = globalThis.fetch;
const originalDateNow = Date.now;
test("available models exclude providers without browser keys", async () => {
  expect(await bridge.availableModels("https://example.com/v1", "openai/gpt-4.1-mini", {})).toEqual(
    [],
  );
});

test("OpenRouter discovery filters the Pi catalog before a prompt", async () => {
  globalThis.fetch = async (url, options) => {
    expect(url).toBe("https://openrouter.ai/api/v1/models/user");
    expect(options.headers.Authorization).toBe("Bearer test-key");
    return Response.json({
      data: [
        { id: "openai/gpt-4.1-mini", supported_parameters: ["tools"] },
        { id: "openrouter/auto", supported_parameters: [] },
      ],
    });
  };
  const available = await bridge.availableModels("", "", { openrouter: "test-key" });
  expect(available.map((model) => model.id)).toEqual(["openrouter/openai/gpt-4.1-mini"]);
});

test("discovery errors do not fall back to an unverified catalog", async () => {
  globalThis.fetch = async () => new Response(null, { status: 401 });
  await expect(bridge.availableModels("", "", { openrouter: "test-key" })).rejects.toThrow(
    "HTTP 401",
  );
});

test("OpenRouter account limits and prices update the picker and the actual request", async () => {
  globalThis.fetch = async () =>
    Response.json({
      data: [
        {
          id: "openai/gpt-4.1-mini",
          supported_parameters: ["tools"],
          context_length: 128_000,
          top_provider: { context_length: 64_000, max_completion_tokens: 1234 },
          pricing: {
            prompt: "0.00000125",
            completion: "0.0000025",
            input_cache_read: "0.000000125",
          },
        },
      ],
    });
  const credentials = { openrouter: "account-pricing" };
  const [model] = await bridge.availableModels("", "", credentials);
  expect(model.context_window).toBe(64_000);
  expect(model.max_tokens).toBe(1234);
  expect(model.cost.input).toBe(1_250_000);
  expect(model.cost.output).toBe(2_500_000);
  expect(model.cost.cache_read).toBe(125_000);
  expect(bridge.cachedModels("", "", credentials)).toEqual([model]);

  let sent;
  globalThis.fetch = async (_url, options) => {
    sent = JSON.parse(options.body);
    return response([{ delta: { role: "assistant", content: "Ready." }, finish_reason: "stop" }]);
  };
  await bridge.run(request({ model: model.id, credential: credentials.openrouter }), {
    send: async () => {},
    recv: async () => {
      throw new Error("Unexpected tool");
    },
  });
  expect(sent.max_tokens ?? sent.max_completion_tokens).toBe(1234);
  await bridge.run(request({ model: model.id, credential: "different-account" }), {
    send: async () => {},
    recv: async () => {
      throw new Error("Unexpected tool");
    },
  });
  expect(sent.max_tokens ?? sent.max_completion_tokens).not.toBe(1234);
  // Account metadata never mutates the bundled catalog or another key's first list.
  expect(bridge.catalog("", "").find((entry) => entry.id === model.id).max_tokens).not.toBe(1234);
  expect(
    bridge
      .cachedModels("", "", { openrouter: "another-account" })
      .find((entry) => entry.id === model.id).max_tokens,
  ).not.toBe(1234);
});

test("malformed account metadata retains safe catalog limits and prices", async () => {
  const bundled = bridge
    .catalog("", "")
    .find((model) => model.id === "openrouter/openai/gpt-4.1-mini");
  globalThis.fetch = async () =>
    Response.json({
      data: [
        null,
        {
          id: "openai/gpt-4.1-mini",
          supported_parameters: ["tools"],
          context_length: -1,
          top_provider: { context_length: 0, max_completion_tokens: "2000" },
          pricing: { prompt: "Infinity", completion: "", input_cache_read: "-1", overrides: null },
        },
        { id: "unknown/new-model", supported_parameters: ["tools"] },
      ],
    });
  const [model] = await bridge.availableModels("", "", { openrouter: "malformed-metadata" });
  expect(model.context_window).toBe(bundled.context_window);
  expect(model.max_tokens).toBe(bundled.max_tokens);
  expect(model.cost).toEqual(bundled.cost);
});

test("account pricing preserves paid prompt tiers without calling zero base rates free", async () => {
  globalThis.fetch = async () =>
    Response.json({
      data: [
        {
          id: "openai/gpt-4.1-mini",
          supported_parameters: ["tools"],
          pricing: {
            prompt: "0",
            completion: "0",
            input_cache_read: "0",
            input_cache_write: "0",
            overrides: [{ min_prompt_tokens: 1000, prompt: "0.000002" }],
          },
        },
      ],
    });
  const [model] = await bridge.availableModels("", "", { openrouter: "tier-pricing" });
  expect(model.cost.input).toBe(0);
  expect(model.cost.output).toBe(0);
  expect(model.cost.has_paid_tier).toBe(true);
});

test("shared account discovery builds each caller's custom model separately", async () => {
  let finish;
  globalThis.fetch = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  const credentials = { openrouter: "shared-with-custom", custom: "custom-key" };
  const first = bridge.availableModels("https://first.invalid/v1", "custom/first", credentials);
  const second = bridge.availableModels("https://second.invalid/v1", "custom/second", credentials);
  finish(Response.json({ data: [] }));
  expect((await first).map((model) => model.id)).toEqual(["custom/first"]);
  expect((await second).map((model) => model.id)).toEqual(["custom/second"]);
});

test("conditional account prices stay distinct from free models when Pi cannot express their schedule", async () => {
  globalThis.fetch = async () =>
    Response.json({
      data: [
        {
          id: "openai/gpt-4.1-mini",
          supported_parameters: ["tools"],
          pricing: {
            prompt: "0",
            completion: "0",
            input_cache_read: "0",
            input_cache_write: "0",
            overrides: [null, {}, { min_prompt_tokens: 0, utc_start: 12, prompt: "0.000002" }],
          },
        },
      ],
    });
  const [model] = await bridge.availableModels("", "", { openrouter: "scheduled-pricing" });
  expect(model.cost.input).toBe(0);
  expect(model.cost.has_paid_tier).toBe(true);
});

test("invalid header credentials are rejected without exposing the secret", async () => {
  const credential = "private-key→invalid";
  globalThis.fetch = () => {
    throw new Error("Must not fetch");
  };
  await expect(bridge.availableModels("", "", { openrouter: credential })).rejects.toThrow(
    "unsupported characters",
  );
  await expect(bridge.run(request({ credential }), {})).rejects.toThrow("unsupported characters");
});
afterEach(() => {
  globalThis.fetch = originalFetch;
  Date.now = originalDateNow;
  bridge.cachedModels("", "", {});
});

test("cached catalog is immediate and never requests the network", () => {
  globalThis.fetch = () => {
    throw new Error("Unexpected discovery");
  };
  const models = bridge.cachedModels("", "", { openrouter: "cached-key" });
  expect(models.length).toBeGreaterThan(1);
  expect(models.every((model) => model.provider === "openrouter")).toBe(true);
});

test("refresh retains cached models on failure and invalidates changed credentials", async () => {
  const credentials = { openrouter: "cache-success" };
  globalThis.fetch = async () =>
    Response.json({ data: [{ id: "openai/gpt-4.1-mini", supported_parameters: ["tools"] }] });
  const refreshed = await bridge.availableModels("", "", credentials);
  expect(bridge.cachedModels("", "", credentials)).toEqual(refreshed);
  const expired = Date.now() + 60_001;
  Date.now = () => expired;
  globalThis.fetch = async () => new Response(null, { status: 503 });
  await expect(bridge.availableModels("", "", credentials)).rejects.toThrow("HTTP 503");
  expect(bridge.cachedModels("", "", credentials)).toEqual(refreshed);
  expect(bridge.cachedModels("", "", { openrouter: "replacement-key" }).length).toBeGreaterThan(1);
  expect(bridge.cachedModels("", "", {})).toEqual([]);
});

test("concurrent refreshes share discovery while cached reads stay available", async () => {
  let finish;
  let calls = 0;
  globalThis.fetch = () => {
    calls += 1;
    return new Promise((resolve) => {
      finish = resolve;
    });
  };
  const credentials = { openrouter: "concurrent-key" };
  const first = bridge.availableModels("", "", credentials);
  const second = bridge.availableModels("", "", credentials);
  expect(bridge.cachedModels("", "", credentials).length).toBeGreaterThan(1);
  expect(calls).toBe(1);
  finish(Response.json({ data: [] }));
  expect(await first).toEqual([]);
  expect(await second).toEqual([]);
  expect(bridge.cachedModels("", "", credentials)).toEqual([]);
});

test("successful discovery has a one-minute cooldown and key changes bypass it", async () => {
  let now = 1_000_000;
  Date.now = () => now;
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return Response.json({ data: [] });
  };
  const credentials = { openrouter: "cooldown-key" };
  await bridge.availableModels("", "", credentials);
  await bridge.availableModels("", "", credentials);
  now += 59_999;
  await bridge.availableModels("", "", credentials);
  expect(calls).toBe(1);
  now += 1;
  await bridge.availableModels("", "", credentials);
  expect(calls).toBe(2);
  await bridge.availableModels("", "", { openrouter: "changed-key" });
  expect(calls).toBe(3);
});

test("a late refresh cannot overwrite a different account's cache", async () => {
  let finish;
  globalThis.fetch = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  const old = bridge.availableModels("", "", { openrouter: "old-key" });
  const current = { openrouter: "new-key" };
  const initial = bridge.cachedModels("", "", current);
  finish(Response.json({ data: [] }));
  await old;
  expect(bridge.cachedModels("", "", current)).toEqual(initial);
});

function request(overrides = {}) {
  return {
    conversationId: crypto.randomUUID(),
    endpoint: "https://provider.invalid/v1/chat/completions",
    model: "custom/test-model",
    credential: "test-key",
    thinkingLevel: "off",
    messages: [],
    history: null,
    prompt: "Read the project",
    images: [],
    priorTokens: 0,
    workspaceInstructions: "",
    maxResponseBytes: 1024 * 1024,
    maxResponseEvents: 8192,
    ...overrides,
  };
}

test("workspace instructions reach the provider before the user message", async () => {
  let sent;
  globalThis.fetch = async (_url, options) => {
    sent = JSON.parse(options.body);
    return response([
      { delta: { role: "assistant", content: "Hello" }, finish_reason: null },
      { delta: {}, finish_reason: "stop" },
    ]);
  };
  await bridge.run(
    request({ workspaceInstructions: "\nAGENTS.md: Always inspect app.js first." }),
    {
      send: async () => {},
      recv: async () => {
        throw new Error("No tool expected");
      },
    },
  );
  const system = sent.messages.find(
    (message) => message.role === "system" || message.role === "developer",
  );
  expect(system.content).toContain("Always inspect app.js first.");
  expect(system.content).toContain("/workspace");
});

test("continuing a chat replaces changed workspace instructions and removes deleted instructions", async () => {
  let sent;
  let history;
  globalThis.fetch = async (_url, options) => {
    sent = JSON.parse(options.body);
    return response([{ delta: { role: "assistant", content: "Ready." }, finish_reason: "stop" }]);
  };
  const input = request();
  const run = async (workspaceInstructions) => {
    await bridge.run(
      { ...input, history, workspaceInstructions },
      {
        send: async (event) => {
          if (event.kind === "completed") history = structuredClone(event.history);
        },
        recv: async () => {
          throw new Error("Unexpected tool");
        },
      },
    );
    return sent.messages
      .filter((message) => message.role === "system" || message.role === "developer")
      .map((message) => message.content)
      .join("\n");
  };
  expect(await run("\nUse the old project rules.")).toContain("old project rules");
  const updated = await run("\nUse the revised project rules.");
  expect(updated).toContain("revised project rules");
  expect(updated).not.toContain("old project rules");
  const removed = await run("");
  expect(removed).not.toContain("project rules");
  expect(removed).toContain("/workspace");
  const systemMessages = history.filter((message) => message.role === "system").length;
  await run("");
  expect(history.filter((message) => message.role === "system")).toHaveLength(systemMessages);
  expect(sent.messages.filter((message) => message.role === "assistant")).toHaveLength(3);
});

test.each([
  ["read", { path: "app.js", offset: 0 }],
  ["edit", { path: "app.js", edits: [] }],
  ["edit", { path: "app.js", edits: Array(101).fill({ old_text: "a", new_text: "b" }) }],
])("Pi rejects invalid %s arguments before calling workspace tools", async (name, args) => {
  let calls = 0;
  const tools = [];
  globalThis.fetch = async () => {
    calls += 1;
    return calls === 1
      ? response([
          {
            delta: {
              role: "assistant",
              tool_calls: [
                {
                  index: 0,
                  id: "invalid-args",
                  type: "function",
                  function: { name, arguments: JSON.stringify(args) },
                },
              ],
            },
            finish_reason: "tool_calls",
          },
        ])
      : response([
          { delta: { role: "assistant", content: "Invalid arguments." }, finish_reason: "stop" },
        ]);
  };
  await bridge.run(request(), {
    send: async (event) => {
      if (event.kind === "tool") tools.push(event);
    },
    recv: async () => {
      throw new Error("Invalid arguments must not reach the workspace");
    },
  });
  expect(calls).toBe(2);
  expect(tools).toEqual([]);
});

test("tool errors produce failed activity instead of successful completion", async () => {
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return calls === 1
      ? response([
          {
            delta: {
              role: "assistant",
              tool_calls: [
                {
                  index: 0,
                  id: "missing-file",
                  type: "function",
                  function: { name: "read", arguments: '{"path":"missing.md"}' },
                },
              ],
            },
            finish_reason: null,
          },
          { delta: {}, finish_reason: "tool_calls" },
        ])
      : response([
          { delta: { role: "assistant", content: "The file is missing." }, finish_reason: null },
          { delta: {}, finish_reason: "stop" },
        ]);
  };
  const events = [];
  await bridge.run(request(), {
    send: async (event) => events.push(event),
    recv: async () => ({ id: "missing-file", error: "File not found" }),
  });
  expect(
    events.some(
      (event) => event.event?.type === "tool_failed" && event.event.id === "missing-file",
    ),
  ).toBe(true);
  expect(
    events.some(
      (event) => event.event?.type === "tool_completed" && event.event.id === "missing-file",
    ),
  ).toBe(false);
});

function response(chunks) {
  const frames =
    chunks
      .map(
        (chunk) =>
          `data: ${JSON.stringify({
            id: "completion",
            object: "chat.completion.chunk",
            created: 0,
            model: "test-model",
            choices: [{ index: 0, ...chunk }],
          })}\n\n`,
      )
      .join("") + "data: [DONE]\n\n";
  return new Response(frames, { headers: { "content-type": "text/event-stream" } });
}

test("catalog uses Pi model capabilities and excludes server-only providers", () => {
  const catalog = bridge.catalog("https://provider.invalid/v1", "custom/test-model");
  expect(catalog.some((model) => model.id === "openai/gpt-4.1-mini")).toBe(true);
  expect(catalog.some((model) => model.reasoning && model.thinking_levels.length > 1)).toBe(true);
  expect(catalog.some((model) => model.supports_images)).toBe(true);
  expect(
    catalog.some((model) =>
      ["openai-codex", "github-copilot", "amazon-bedrock"].includes(model.provider),
    ),
  ).toBe(false);
  expect(catalog.find((model) => model.id === "custom/test-model")?.thinking_levels).toEqual([
    "off",
  ]);
});

test("OpenRouter variable prices are valid unsigned catalog values, not free models", () => {
  const catalog = bridge.catalog("https://provider.invalid/v1", "custom/test-model");
  for (const model of catalog) {
    for (const field of ["input", "output", "cache_read", "cache_write"]) {
      expect(Number.isSafeInteger(model.cost[field])).toBe(true);
      expect(model.cost[field]).toBeGreaterThanOrEqual(0);
    }
  }
  for (const id of [
    "openrouter/openrouter/auto",
    "openrouter/openrouter/auto-beta",
    "openrouter/openrouter/auto-code",
  ]) {
    const model = catalog.find((model) => model.id === id);
    expect(model).toBeDefined();
    expect(model.cost.input).toBe(0);
    expect(model.cost.has_paid_tier).toBe(true);
  }
});

test("Pi executes a tool and preserves its transcript for the next prompt", async () => {
  const calls = [];
  globalThis.fetch = async (url, options) => {
    expect(String(url)).toBe("https://provider.invalid/v1/chat/completions");
    expect(new Headers(options.headers).get("authorization")).toBe("Bearer test-key");
    const body = JSON.parse(options.body);
    calls.push(body);
    return calls.length === 1
      ? response([
          {
            delta: {
              role: "assistant",
              tool_calls: [
                {
                  index: 0,
                  id: "read-1",
                  type: "function",
                  function: { name: "read", arguments: '{"path":"README.md"}' },
                },
              ],
            },
            finish_reason: null,
          },
          { delta: {}, finish_reason: "tool_calls" },
        ])
      : response([
          { delta: { role: "assistant", content: "Project inspected." }, finish_reason: null },
          { delta: {}, finish_reason: "stop" },
        ]);
  };
  const events = [];
  let tool;
  await bridge.run(request(), {
    send: async (event) => {
      events.push(structuredClone(event));
      if (event.kind === "tool") tool = event;
    },
    recv: async () => ({ id: tool.id, output: "# Fixture project" }),
  });
  expect(tool.name).toBe("read");
  expect(calls).toHaveLength(2);
  expect(
    calls[1].messages.some(
      (message) => message.role === "tool" && message.content.includes("Fixture project"),
    ),
  ).toBe(true);
  expect(
    events.some(
      (event) =>
        event.event?.type === "assistant_completed" && event.event.content === "Project inspected.",
    ),
  ).toBe(true);
  const history = events.at(-1).history;
  expect(history.some((message) => message.role === "toolResult")).toBe(true);
  const completed = events.find((event) => event.event?.type === "tool_completed").event;
  expect(Number.isSafeInteger(completed.duration_ms)).toBe(true);
  expect(completed.duration_ms).toBeGreaterThanOrEqual(0);
  expect(history.find((message) => message.role === "toolResult").durationMs).toBe(
    completed.duration_ms,
  );
  await bridge.run(request({ history, prompt: "Continue" }), {
    send: async () => {},
    recv: async () => {
      throw new Error("Unexpected tool");
    },
  });
  expect(calls.at(-1).messages.some((message) => message.role === "tool")).toBe(true);
  expect(JSON.stringify(history)).not.toContain("test-key");
});

test("response limits produce an error and settle the session", async () => {
  globalThis.fetch = async () =>
    response([{ delta: { role: "assistant", content: "x".repeat(4096) }, finish_reason: "stop" }]);
  const events = [];
  await bridge.run(request({ maxResponseBytes: 128 }), {
    send: async (event) => events.push(event),
    recv: async () => {
      throw new Error("Unexpected tool");
    },
  });
  expect(events.some((event) => event.event?.type === "failed")).toBe(true);
  expect(events.at(-1).kind).toBe("completed");
});

test("cancellation settles even when a tool response never arrives", async () => {
  globalThis.fetch = async () =>
    response([
      {
        delta: {
          role: "assistant",
          tool_calls: [
            {
              index: 0,
              id: "read-1",
              type: "function",
              function: { name: "read", arguments: '{"path":"README.md"}' },
            },
          ],
        },
        finish_reason: null,
      },
      { delta: {}, finish_reason: "tool_calls" },
    ]);
  const input = request();
  const events = [];
  await bridge.run(input, {
    send: async (event) => {
      events.push(event);
      if (event.kind === "tool") bridge.abort(input.conversationId);
    },
    recv: async () => new Promise(() => {}),
  });
  expect(events.at(-1).kind).toBe("completed");
});

test("Pi stops at 20 turns and retains the last tool result for continuation", async () => {
  let calls = 0;
  let tool;
  const events = [];
  globalThis.fetch = async () => {
    calls += 1;
    return response([
      {
        delta: {
          role: "assistant",
          tool_calls: [
            {
              index: 0,
              id: `read-${calls}`,
              type: "function",
              function: { name: "read", arguments: '{"path":"README.md","offset":2,"limit":1}' },
            },
          ],
        },
        finish_reason: null,
      },
      { delta: {}, finish_reason: "tool_calls" },
    ]);
  };
  const input = request();
  await bridge.run(input, {
    send: async (event) => {
      events.push(structuredClone(event));
      if (event.kind === "tool") tool = event;
    },
    recv: async () => ({ id: tool.id, output: "2: project\n[Lines 2-2 of 3]" }),
  });
  expect(calls).toBe(20);
  expect(tool.args).toEqual({ path: "README.md", offset: 2, limit: 1 });
  expect(events.some((event) => event.event?.message?.includes("20 agent turns"))).toBe(true);
  expect(events.at(-1).kind).toBe("completed");
  const history = events.at(-1).history;
  expect(history.at(-1).role).toBe("toolResult");
  expect(history.at(-1).toolCallId).toBe("read-20");
  globalThis.fetch = async (_url, options) => {
    const body = JSON.parse(options.body);
    expect(body.messages.some((message) => message.tool_call_id === "read-20")).toBe(true);
    return response([
      { delta: { role: "assistant", content: "Continued." }, finish_reason: "stop" },
    ]);
  };
  await bridge.run(
    { ...input, history, prompt: "Continue" },
    {
      send: async () => {},
      recv: async () => {
        throw new Error("Unexpected tool");
      },
    },
  );
});

test("Pi validates and forwards batch edit arguments through the workspace channel", async () => {
  let calls = 0;
  let tool;
  const edits = [
    { old_text: "one", new_text: "two" },
    { old_text: "two", new_text: "three" },
  ];
  globalThis.fetch = async (_url, options) => {
    const body = JSON.parse(options.body);
    expect(
      body.tools.find((tool) => tool.function.name === "read").function.parameters.properties
        .offset,
    ).toBeDefined();
    expect(
      body.tools.find((tool) => tool.function.name === "edit").function.parameters.properties.edits,
    ).toBeDefined();
    calls += 1;
    return calls === 1
      ? response([
          {
            delta: {
              role: "assistant",
              tool_calls: [
                {
                  index: 0,
                  id: "batch-edit",
                  type: "function",
                  function: { name: "edit", arguments: JSON.stringify({ path: "app.js", edits }) },
                },
              ],
            },
            finish_reason: null,
          },
          { delta: {}, finish_reason: "tool_calls" },
        ])
      : response([{ delta: { role: "assistant", content: "Edited." }, finish_reason: "stop" }]);
  };
  await bridge.run(request(), {
    send: async (event) => {
      if (event.kind === "tool") tool = event;
    },
    recv: async () => ({ id: tool.id, output: "Edited app.js" }),
  });
  expect(calls).toBe(2);
  expect(tool.args).toEqual({ path: "app.js", edits });
});

test("provider cache routing is stable within a chat and separate between chats", async () => {
  const cacheKeys = [];
  globalThis.fetch = async (_url, options) => {
    cacheKeys.push(JSON.parse(options.body).prompt_cache_key);
    return response([{ delta: { role: "assistant", content: "Hello." }, finish_reason: "stop" }]);
  };
  const input = request({ endpoint: "https://api.openai.com/v1" });
  const channel = {
    send: async () => {},
    recv: async () => {
      throw new Error("Unexpected tool");
    },
  };
  await bridge.run(input, channel);
  await bridge.run({ ...input, prompt: "Continue" }, channel);
  await bridge.run({ ...input, conversationId: crypto.randomUUID() }, channel);
  expect(typeof cacheKeys[0]).toBe("string");
  expect(cacheKeys[0].length).toBeGreaterThan(0);
  expect(cacheKeys[1]).toBe(cacheKeys[0]);
  expect(cacheKeys[2]).not.toBe(cacheKeys[0]);
  expect(cacheKeys.join()).not.toContain("test-key");
});
