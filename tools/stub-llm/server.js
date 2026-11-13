const http = require("http");

const PORT = Number(process.env.PORT || 4098);
const MODEL = process.env.MODEL || "stub-model";

function sse(res, obj) {
  res.write(`data: ${JSON.stringify(obj)}\n\n`);
}

function chunk(delta, finish) {
  return {
    id: "chatcmpl-stub",
    object: "chat.completion.chunk",
    created: Math.floor(Date.now() / 1000),
    model: MODEL,
    choices: [{ index: 0, delta, finish_reason: finish || null }],
  };
}

function completion(message, finish) {
  return {
    id: "chatcmpl-stub",
    object: "chat.completion",
    created: Math.floor(Date.now() / 1000),
    model: MODEL,
    choices: [{ index: 0, message, finish_reason: finish }],
    usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
  };
}

function pickTool(body) {
  const tools = body.tools || [];
  const names = tools.map((t) => t.function && t.function.name).filter(Boolean);
  const preferred = names.find((n) => n.endsWith("resolve-library-id")) || names[0];
  const args = preferred && preferred.endsWith("resolve-library-id")
    ? { libraryName: "Tauri", query: "command argument naming" }
    : {};
  return { name: preferred, args };
}

const server = http.createServer((req, res) => {
  if (req.method === "GET" && req.url.startsWith("/v1/models")) {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ object: "list", data: [{ id: MODEL, object: "model" }] }));
    return;
  }

  if (req.method !== "POST" || !req.url.startsWith("/v1/chat/completions")) {
    res.writeHead(404, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: { message: "not found" } }));
    return;
  }

  if (req.url.includes("chat/completions")) console.log("auth header:", req.headers.authorization || "(none)");
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    let parsed = {};
    try {
      parsed = JSON.parse(body || "{}");
    } catch (e) {
      res.writeHead(400, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ error: { message: "bad json" } }));
      return;
    }

    const messages = parsed.messages || [];
    const hasToolResult = messages.some((m) => m.role === "tool");
    const streaming = Boolean(parsed.stream);

    let message;
    let finish;
    if (!hasToolResult && (parsed.tools || []).length > 0) {
      const { name, args } = pickTool(parsed);
      message = {
        role: "assistant",
        content: null,
        tool_calls: [
          {
            id: "call_stub_1",
            type: "function",
            function: { name, arguments: JSON.stringify(args) },
          },
        ],
      };
      finish = "tool_calls";
    } else if (hasToolResult) {
      const last = messages[messages.length - 1];
      const preview = String(last.content || "").slice(0, 160).replace(/\s+/g, " ");
      message = { role: "assistant", content: `[ДЕМО-ЗАГЛУШКА] Это не ответ модели. Заглушка вызвала первый доступный инструмент и вернула его результат: ${preview}` };
      finish = "stop";
    } else {
      message = { role: "assistant", content: `[ДЕМО-ЗАГЛУШКА] Это не ответ модели, а тестовый сервер. Укажите реальный endpoint в настройках.` };
      finish = "stop";
    }

    if (!streaming) {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(completion(message, finish)));
      return;
    }

    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    });

    if (finish === "tool_calls") {
      const call = message.tool_calls[0];
      sse(res, chunk({ role: "assistant", tool_calls: [{ index: 0, id: call.id, type: "function", function: { name: call.function.name, arguments: "" } }] }));
      sse(res, chunk({ tool_calls: [{ index: 0, function: { arguments: call.function.arguments } }] }));
      sse(res, chunk({}, "tool_calls"));
    } else {
      sse(res, chunk({ role: "assistant", content: message.content }));
      sse(res, chunk({}, "stop"));
    }
    res.write("data: [DONE]\n\n");
    res.end();
  });
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`stub LLM listening on http://127.0.0.1:${PORT}/v1`);
});
