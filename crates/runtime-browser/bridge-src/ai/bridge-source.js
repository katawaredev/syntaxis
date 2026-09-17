import { Agent } from "@earendil-works/pi-agent-core";
import {
  Type,
  createModels,
  createProvider,
  getSupportedThinkingLevels,
  clampThinkingLevel,
  lazyApi,
} from "@earendil-works/pi-ai";
import { openaiProvider } from "@earendil-works/pi-ai/providers/openai";
import { anthropicProvider } from "@earendil-works/pi-ai/providers/anthropic";
import { googleProvider } from "@earendil-works/pi-ai/providers/google";
import { mistralProvider } from "@earendil-works/pi-ai/providers/mistral";
import { groqProvider } from "@earendil-works/pi-ai/providers/groq";
import { openrouterProvider } from "@earendil-works/pi-ai/providers/openrouter";
import { xaiProvider } from "@earendil-works/pi-ai/providers/xai";

// Explicit factories keep Node-only and OAuth providers out of this browser runtime.
const models = createModels();
for (const factory of [
  openaiProvider,
  anthropicProvider,
  googleProvider,
  mistralProvider,
  groqProvider,
  openrouterProvider,
  xaiProvider,
]) {
  models.setProvider(factory());
}
const requests = new Map();
function validateCredential(key) {
  if (typeof key !== "string" || !/^[\x21-\x7e]+$/.test(key)) {
    throw new Error(
      "The provider API key contains whitespace or unsupported characters. Re-enter only the API key in Settings > Provider accounts.",
    );
  }
}

let accountModels;
let accountKey;
let pendingDiscovery;
let accountGeneration = 0;
let accountRefreshedAt = 0;
const MODEL_REFRESH_COOLDOWN_MS = 60_000;

function cachedModels(endpoint, defaultModel, credentials) {
  if (accountKey !== credentials.openrouter) {
    accountKey = credentials.openrouter;
    accountModels = undefined;
    pendingDiscovery = undefined;
    accountGeneration += 1;
    accountRefreshedAt = 0;
  }
  return catalog(endpoint, defaultModel).filter(
    (model) =>
      Boolean(credentials[model.provider]) &&
      (model.provider !== "openrouter" || !accountModels || accountModels.has(model.id)),
  );
}

async function availableModels(endpoint, defaultModel, credentials) {
  cachedModels(endpoint, defaultModel, credentials);
  const key = credentials.openrouter;
  const generation = accountGeneration;
  if (accountRefreshedAt && Date.now() - accountRefreshedAt < MODEL_REFRESH_COOLDOWN_MS) {
    return cachedModels(endpoint, defaultModel, credentials);
  }
  if (!pendingDiscovery) {
    const discovery = discoverModels(endpoint, defaultModel, credentials);
    pendingDiscovery = discovery;
    discovery
      .finally(() => {
        if (pendingDiscovery === discovery) pendingDiscovery = undefined;
      })
      .catch(() => {});
  }
  const available = await pendingDiscovery;
  if (accountKey === key && accountGeneration === generation) {
    accountModels = new Set(
      available.filter((model) => model.provider === "openrouter").map((model) => model.id),
    );
    accountRefreshedAt = Date.now();
  } else {
    return available;
  }
  // Build each caller's result from its own custom model and configured providers.
  return cachedModels(endpoint, defaultModel, credentials);
}

async function discoverModels(endpoint, defaultModel, credentials) {
  const configured = new Set(Object.keys(credentials));
  let available = catalog(endpoint, defaultModel).filter((model) => configured.has(model.provider));
  if (credentials.openrouter) {
    validateCredential(credentials.openrouter);
    let response;
    try {
      response = await fetch("https://openrouter.ai/api/v1/models/user", {
        headers: { Authorization: `Bearer ${credentials.openrouter}` },
        signal: AbortSignal.timeout(15000),
      });
    } catch {
      throw new Error(
        "Could not reach OpenRouter to load account models. Check your connection and retry.",
      );
    }
    if (!response.ok) {
      throw new Error(
        `OpenRouter model discovery failed (HTTP ${response.status}). Check your API key and account settings.`,
      );
    }
    const result = await response.json();
    if (!Array.isArray(result.data)) throw new Error("OpenRouter returned an invalid model list.");
    // Intersect live account availability with models this installed Pi version supports.
    const allowed = new Set(
      result.data
        .filter(
          (model) =>
            Array.isArray(model.supported_parameters) &&
            model.supported_parameters.includes("tools"),
        )
        .map((model) => `openrouter/${model.id}`),
    );
    available = available.filter(
      (model) => model.provider !== "openrouter" || allowed.has(model.id),
    );
  }
  return available;
}
const zeroCost = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
const zeroUsage = { ...zeroCost, totalTokens: 0, cost: { ...zeroCost, total: 0 } };

function customModel(endpoint, id) {
  return {
    id,
    name: id,
    provider: "custom",
    api: "openai-completions",
    baseUrl: endpoint.replace(/\/chat\/completions\/?$/, "").replace(/\/$/, ""),
    reasoning: false,
    input: ["text"],
    cost: zeroCost,
    contextWindow: 0,
    maxTokens: 4096,
  };
}

function resolveModel(key, endpoint) {
  if (key.startsWith("custom/")) return customModel(endpoint, key.slice(7));
  const separator = key.indexOf("/");
  const model = models.getModel(key.slice(0, separator), key.slice(separator + 1));
  if (!model) throw new Error("Choose an available model in AI settings.");
  return model;
}

function catalogRate(value) {
  const rate = Math.round(value * 1_000_000);
  return Number.isSafeInteger(rate) && rate >= 0 ? rate : 0;
}

function catalog(endpoint, defaultModel) {
  const available = [...models.getModels()];
  if (defaultModel.startsWith("custom/"))
    available.push(customModel(endpoint, defaultModel.slice(7)));
  return available.map((model) => ({
    id: `${model.provider}/${model.id}`,
    label: model.name,
    provider: model.provider,
    reasoning: model.reasoning,
    thinking_levels: getSupportedThinkingLevels(model),
    supports_images: model.input.includes("image"),
    context_window: model.contextWindow,
    max_tokens: model.maxTokens,
    cost: {
      input: catalogRate(model.cost.input),
      output: catalogRate(model.cost.output),
      cache_read: catalogRate(model.cost.cacheRead),
      cache_write: catalogRate(model.cost.cacheWrite),
      // Routers use negative prices when their rate depends on the chosen model.
      // Keep that distinct from a genuinely free model in the unsigned Rust schema.
      has_paid_tier:
        model.provider === "custom" ||
        Object.values(model.cost).some(
          (value) => !Number.isSafeInteger(Math.round(value * 1_000_000)) || value < 0,
        ),
    },
  }));
}

const toolDefinitions = [
  [
    "read",
    "Read a UTF-8 workspace file (up to 256 KiB) and applicable directory instructions. Paths may be workspace-relative or start with /workspace/.",
    Type.Object({ path: Type.String() }),
  ],
  [
    "list",
    "List entries in a workspace directory. Use an empty path or /workspace for the root.",
    Type.Object({ path: Type.String() }),
  ],
  [
    "write",
    "Create or replace a UTF-8 workspace file (up to 256 KiB). Parent directories must exist.",
    Type.Object({ path: Type.String(), content: Type.String() }),
  ],
  [
    "edit",
    "Replace one exact, unique text occurrence in a workspace file.",
    Type.Object({ path: Type.String(), old_text: Type.String(), new_text: Type.String() }),
  ],
  [
    "bash",
    "Run a command in the browser's just-bash sandbox. This is not a native shell: no native processes, package installation, or unrestricted network access.",
    Type.Object({ command: Type.String() }),
  ],
];

async function receiveTool(channel, signal) {
  let onAbort;
  try {
    return await Promise.race([
      channel.recv(),
      new Promise((_, reject) => {
        onAbort = () => reject(new Error("Cancelled"));
        if (signal?.aborted) onAbort();
        else signal?.addEventListener("abort", onAbort, { once: true });
      }),
    ]);
  } finally {
    if (onAbort) signal?.removeEventListener("abort", onAbort);
  }
}

async function run(request, channel) {
  validateCredential(request.credential);
  if (requests.has(request.conversationId)) throw new Error("This chat is already running.");
  const model = resolveModel(request.model, request.endpoint);
  if (model.provider === "custom") {
    models.setProvider(
      createProvider({
        id: "custom",
        name: "Custom OpenAI-compatible",
        auth: openaiProvider().auth,
        models: [model],
        api: lazyApi(() => import("@earendil-works/pi-ai/api/openai-completions")),
      }),
    );
  }
  let eventCount = 0;
  let bytes = 0;
  let turns = 0;
  let assistantId = crypto.randomUUID();
  let cancelled = false;
  let inputTokens = request.priorTokens ?? 0;
  let outputTokens = 0;
  const send = async (event) => {
    eventCount += 1;
    bytes += new TextEncoder().encode(JSON.stringify(event)).length;
    if (eventCount > request.maxResponseEvents || bytes > request.maxResponseBytes) {
      throw new Error("The browser AI response limit was reached.");
    }
    await channel.send({ kind: "event", event });
  };
  const agent = new Agent({
    initialState: {
      model,
      thinkingLevel: clampThinkingLevel(model, request.thinkingLevel),
      systemPrompt:
        "You are the Syntaxis coding assistant. Work only in the user's browser workspace using the provided tools. The workspace root is /workspace. File tools accept relative paths or /workspace/...; bash starts in /workspace on each call. Read files and applicable directory AGENTS.md instructions before editing. The bash tool is a limited browser sandbox, not a server terminal. Explain unsupported operations honestly. Workspace instructions below were loaded automatically for this turn. Markdown skills are guidance, not executable extensions. Do not claim that code was tested unless a tool actually ran the check." +
        (request.workspaceInstructions ?? ""),
      messages:
        request.history ??
        request.messages
          .filter((message) => message.role !== "system")
          .map((message) =>
            message.role === "user"
              ? {
                  role: "user",
                  content: [{ type: "text", text: message.content }],
                  timestamp: Date.now(),
                }
              : {
                  role: "assistant",
                  content: [{ type: "text", text: message.content }],
                  api: model.api,
                  provider: model.provider,
                  model: model.id,
                  usage: zeroUsage,
                  stopReason: "stop",
                  timestamp: Date.now(),
                },
          ),
      tools: toolDefinitions.map(([name, description, parameters]) => ({
        name,
        label: name,
        description,
        parameters,
        execute: async (id, args, signal) => {
          if (signal?.aborted) throw new Error("Cancelled");
          await channel.send({ kind: "tool", id, name, args });
          const result = await receiveTool(channel, signal);
          if (signal?.aborted) throw new Error("Cancelled");
          if (result.id !== id) throw new Error("Unexpected tool response.");
          if (result.error) throw new Error(result.error);
          return { content: [{ type: "text", text: result.output }], details: {} };
        },
      })),
    },
    streamFn: (selected, context, options) =>
      models.streamSimple(selected, context, { ...options, apiKey: request.credential }),
    toolExecution: "sequential",
    shouldStopAfterTurn: () => ++turns >= 20,
  });
  requests.set(request.conversationId, {
    abort: () => {
      cancelled = true;
      agent.abort();
    },
  });
  agent.subscribe(async (event) => {
    if (event.type === "message_end") {
      await channel.send({ kind: "history", history: agent.state.messages });
    }
    if (event.type === "message_start" && event.message.role === "assistant")
      assistantId = crypto.randomUUID();
    if (event.type === "message_update") {
      const delta = event.assistantMessageEvent;
      if (delta.type === "text_delta" || delta.type === "thinking_delta") {
        await send({
          type: delta.type === "text_delta" ? "assistant_delta" : "assistant_thinking_delta",
          message_id: assistantId,
          text: delta.delta,
        });
      }
    }
    if (event.type === "message_end" && event.message.role === "assistant") {
      const message = event.message;
      await send({
        type: "assistant_completed",
        id: assistantId,
        role: "assistant",
        content: message.content
          .filter((part) => part.type === "text")
          .map((part) => part.text)
          .join(""),
        thinking: message.content
          .filter((part) => part.type === "thinking")
          .map((part) => part.thinking)
          .join(""),
        status:
          message.stopReason === "error"
            ? "failed"
            : message.stopReason === "aborted"
              ? "stopped"
              : "complete",
      });
      inputTokens += message.usage.input + message.usage.cacheRead + message.usage.cacheWrite;
      outputTokens += message.usage.output;
      await send({ type: "usage_updated", input_tokens: inputTokens, output_tokens: outputTokens });
      if (message.errorMessage) await send({ type: "failed", message: message.errorMessage });
    }
    if (event.type === "tool_execution_start")
      await send({ type: "tool_started", id: event.toolCallId, name: event.toolName });
    if (event.type === "tool_execution_end") {
      await send({
        type: event.isError ? "tool_failed" : "tool_completed",
        id: event.toolCallId,
        output:
          (event.isError ? "Tool failed: " : "") +
          (event.result.content
            ?.filter((part) => part.type === "text")
            .map((part) => part.text)
            .join("\n") ?? ""),
      });
    }
  });
  try {
    await agent.prompt(
      request.prompt,
      request.images.map((image) => ({
        type: "image",
        data: image.data,
        mimeType: image.mime_type,
      })),
    );
    if (turns >= 20)
      await send({
        type: "failed",
        message: "Stopped after 20 agent turns. Send another message to continue.",
      });
  } catch (error) {
    await channel.send({
      kind: "event",
      event: { type: "failed", message: error?.message ?? "The Pi request failed." },
    });
  } finally {
    requests.delete(request.conversationId);
    if (cancelled) {
      await channel.send({
        kind: "event",
        event: { type: "failed", message: "The AI request was cancelled." },
      });
    }
    await channel.send({ kind: "completed", history: agent.state.messages });
  }
}

globalThis.SyntaxisBrowserAi = {
  version: 1,
  catalog,
  cachedModels,
  availableModels,
  run,
  abort: (id) => requests.get(id)?.abort(),
};
