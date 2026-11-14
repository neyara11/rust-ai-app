const browserMock = (() => {
  const mockApps = [
    {
      app_id: "doc-writer",
      name: "Документы и Word",
      version: "0.1.0",
      publisher: "CN=ECO AIOps Dev",
      path: "D:\\git\\rust-ai-app\\.data\\apps\\doc-writer",
      trusted: true,
      problems: [],
      tools: [{ tool: "docx_export", bin: "bin/docx-export.exe", stream: "write" }],
      capabilities: { file_scopes: ["workspace"] },
      connections: [{ id: "llm", type: "llm", required: true }],
    },
    {
      app_id: "doc-lookup",
      name: "Справка по библиотекам",
      version: "0.9.2",
      publisher: "CN=ECO AIOps Security",
      path: "D:\\git\\rust-ai-app\\.data\\apps\\doc-lookup",
      trusted: true,
      problems: [],
      tools: [
        { tool: "resolve-library-id", argv: [], stream: "read" },
        { tool: "query-docs", argv: [], stream: "read" },
      ],
      connections: [
        { id: "llm", type: "llm", required: true },
        { id: "context7", type: "http-mcp", required: true },
      ],
    },
    {
      app_id: "incident-brief",
      name: "Экспорт в внешний трекер",
      version: "0.3.1",
      publisher: "CN=Untrusted Vendor",
      path: "D:\\git\\rust-ai-app\\.data\\apps\\incident-brief",
      trusted: false,
      problems: ["hash mismatch for prompt/system.md"],
      tools: [{ tool: "push-ticket", argv: [], stream: "write" }],
      connections: [{ id: "llm", type: "llm", required: true }],
    },
  ];

  const mockChats = {
    "doc-writer": [
      {
        id: "ses-301",
        title: "report.md → Word",
        items: [
          {
            role: "user",
            kind: "text",
            text: "В рабочей папке создай report.md с планом проверки гипотезы о деградации БД и сконвертируй его в report.docx.",
          },
          {
            role: "assistant",
            kind: "reasoning",
            text: "Сначала посмотрю файлы в рабочей папке, затем запишу отчёт и вызову `docx_export`.",
          },
          {
            role: "tool",
            kind: "tool",
            text: "platform_list_files -> completed",
          },
          {
            role: "tool",
            kind: "tool",
            text: "platform_write_file -> completed",
          },
          {
            role: "tool",
            kind: "tool",
            text: "platform_docx_export -> completed",
          },
          {
            role: "assistant",
            kind: "text",
            text: "### Готово\n\nСоздал в рабочей папке:\n\n- `report.md` — план проверки\n- `report.docx` — Word-версия через `docx_export`\n\nФайлы можно править и пересобирать.",
            tokens_in: 980,
            tokens_out: 210,
            reasoning_tokens: 120,
            tok_s: 58.4,
            ms: 3400,
            model: "~deepseek/deepseek-v4-flash-latest",
          },
        ],
      },
    ],
    "doc-lookup": [
      {
        id: "ses-201",
        title: "Разграничение прав в Tauri v2",
        items: [],
      },
    ],
  };

  let activeSession = { "doc-writer": "ses-301", "doc-lookup": "ses-201" };
  let secrets = ["llm_api_key"];
  let mcpServers = ["context7"];
  let config = {
    llm_endpoint: "https://routerai.ru/api/v1",
    llm_model: "~deepseek/deepseek-v4-flash-latest",
    llm_capabilities: {
      vision: false,
      reasoning: true,
      reasoning_efforts: ["low", "medium", "high"],
      context_tokens: 128000,
      output_tokens: 32000,
    },
    session_quota_mb: 500,
    global_quota_mb: 5120,
    ttl_hours: 168,
  };

  return async function invokeMock(cmd, args = {}) {
    switch (cmd) {
      case "platform_info":
        return {
          home: "D:\\git\\rust-ai-app\\.data",
          version: "0.1.0",
          schema: 2,
          trusted_anchor: true,
          anchor_fingerprint: "48efcf9d72861a68",
          anchor_fingerprints: ["48efcf9d72861a68"],
          anchor_source: "dev",
          anchor_count: 1,
          allowlist_status: "absent",
          allowlist_source: "none",
          allowlist_apps: 0,
          allowlist_revoked: 0,
          allowlist_expires_at: null,
          allowlist_problem: null,
          dev_mode: true,
        };
      case "list_apps":
        return mockApps;
      case "get_config":
        return config;
      case "save_config":
        config = { ...config, ...(args.config || {}) };
        return null;
      case "secrets_list":
        return secrets;
      case "secret_store":
        if (args.name && !secrets.includes(args.name)) secrets.push(args.name);
        return null;
      case "secret_delete":
        secrets = secrets.filter((s) => s !== args.name);
        return null;
      case "mcp_servers":
        return mcpServers;
      case "mcp_server_save":
        if (args.id && !mcpServers.includes(args.id)) mcpServers.push(args.id);
        return null;
      case "mcp_server_delete":
        mcpServers = mcpServers.filter((s) => s !== args.id);
        return null;
      case "mcp_probe":
        return {
          protocol: "2025-06-18",
          server: { name: args.id },
          tools: ["resolve-library-id", "query-docs"],
        };
      case "mcp_call":
        return {
          bytes: 482,
          truncated: false,
          result: JSON.stringify(
            {
              summary: `Вызов ${args.tool} через ${args.server_id} выполнен успешно`,
              arguments: args.arguments,
              took_ms: 142,
              status: "ok",
            },
            null,
            2
          ),
        };
      case "probe_endpoint":
        return { ok: true, millis: 38, detail: "TCP connect ok (routerai.ru:443)" };
      case "open_app":
        return {
          app_id: args.app_id,
          workspace: `D:\\work\\${args.app_id}`,
        };
      case "choose_workspace":
        return `D:\\work\\${args.app_id}`;
      case "close_app":
      case "close_all_apps":
      case "chat_stop":
      case "chat_abort":
        return null;
      case "artifacts_usage":
        return { sessions: 2, bytes: 18454937, files: 6 };
      case "artifact_dumps":
        return [
          { session: "s1700000100", path: "D:\\mock\\s1700000100\\dumps.ndjson", bytes: 6291456, modified: 1700000100 },
          { session: "s1700000000", path: "D:\\mock\\s1700000000\\dumps.ndjson", bytes: 14486, modified: 1700000000 },
        ];
      case "artifact_grep":
        return [
          {
            session: args.session || "s1700000000",
            path: "D:\\mock\\dumps.ndjson",
            line: 1,
            text: '{"tool":"platform_read_file","result":{"text":"mock hit for ' + String(args.query || "") + '"}}',
          },
        ];
      case "export_artifacts":
        return { dir: "D:\\exports", files: args.session ? 1 : 2, bytes: args.session ? 14486 : 6305942 };
      case "cleanup_artifacts":
        return { removed: 1, bytes_freed: 6291456 };
      case "chat_open":
        return {
          model: config.llm_model,
          session: activeSession[args.app_id] || "ses-101",
        };
      case "chat_list":
        return (mockChats[args.app_id] || []).map((c) => ({ id: c.id, title: c.title }));
      case "chat_history": {
        const sessions = mockChats[args.app_id] || [];
        const current = sessions.find((s) => s.id === activeSession[args.app_id]) || sessions[0];
        return current ? current.items : [];
      }
      case "chat_status":
        return { busy: false };
      case "chat_select":
        activeSession[args.app_id] = args.session_id;
        return { model: config.llm_model, session: args.session_id };
      case "chat_new": {
        const id = "ses-" + Math.floor(Math.random() * 900 + 100);
        if (!mockChats[args.app_id]) mockChats[args.app_id] = [];
        mockChats[args.app_id].unshift({ id, title: "Новый диалог", items: [] });
        activeSession[args.app_id] = id;
        return { model: config.llm_model, session: id };
      }
      case "chat_delete": {
        const list = (mockChats[args.app_id] || []).filter((c) => c.id !== args.session_id);
        mockChats[args.app_id] = list;
        activeSession[args.app_id] = list[0] ? list[0].id : "ses-new";
        return { model: config.llm_model, session: activeSession[args.app_id] };
      }
      case "chat_send": {
        const list = mockChats[args.app_id] || [];
        const current = list.find((s) => s.id === activeSession[args.app_id]);
        if (current) {
          if (current.items.length === 0) current.title = String(args.text).slice(0, 38);
          current.items.push({ role: "user", kind: "text", text: args.text });
          current.items.push({
            role: "assistant",
            kind: "text",
            text: `Принято в обработку в изолированной сессии **${args.app_id}**. Вызов инструментов выполняется строго по allow-list манифеста.`,
            tokens_in: 310,
            tokens_out: 94,
            tok_s: 58.4,
            ms: 1400,
            model: config.llm_model,
          });
        }
        return null;
      }
      default:
        return null;
    }
  };
})();

const invoke = (cmd, args) => {
  if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
    return window.__TAURI__.core.invoke(cmd, args || {});
  }
  return browserMock(cmd, args || {});
};

const $ = (id) => document.getElementById(id);

const ctx = {
  apps: [],
  installed: null,
  appId: null,
  appMeta: null,
  config: null,
  attachments: [],
};

const THEME_KEY = "theme";

function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  const toggle = $("theme-toggle");
  if (toggle) {
    const next = theme === "dark" ? "светлую" : "тёмную";
    toggle.title = `Включить ${next} тему`;
    toggle.setAttribute("aria-label", `Включить ${next} тему`);
  }
}

function initTheme() {
  let saved = null;
  try {
    saved = localStorage.getItem(THEME_KEY);
  } catch (_) {}
  const dark = saved ? saved === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
  setTheme(dark ? "dark" : "light");

  const toggle = $("theme-toggle");
  if (toggle) {
    toggle.addEventListener("click", () => {
      const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
      setTheme(next);
      try {
        localStorage.setItem(THEME_KEY, next);
      } catch (_) {}
    });
  }

  const media = matchMedia("(prefers-color-scheme: dark)");
  const onChange = (event) => {
    let explicit = null;
    try {
      explicit = localStorage.getItem(THEME_KEY);
    } catch (_) {}
    if (!explicit) setTheme(event.matches ? "dark" : "light");
  };
  if (media.addEventListener) media.addEventListener("change", onChange);
}

function appInitials(name) {
  const words = String(name || "")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  const letters = words
    .slice(0, 2)
    .map((word) => word[0])
    .join("");
  return (letters || "AI").toUpperCase();
}

function esc(v) {
  return String(v == null ? "" : v).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[c]));
}

let toastTimer = null;
function toast(message, isError) {
  const el = $("toast");
  if (!el) return;
  el.textContent = message;
  el.className = "toast" + (isError ? " error" : "");
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.hidden = true;
  }, isError ? 8000 : 3200);
}

async function guard(fn) {
  try {
    await fn();
  } catch (e) {
    toast(String(e), true);
  }
}

/* ---------- apps ---------- */

async function refreshApps() {
  ctx.apps = await invoke("list_apps");
  renderApps();
  renderArtifactApps();
}

function renderApps() {
  const box = $("apps");
  const countEl = $("apps-count");
  if (countEl) countEl.textContent = String((ctx.apps || []).length);
  box.innerHTML = "";

  if (!ctx.apps.length) {
    box.innerHTML = `
      <div class="empty-catalog">
        <div class="title">Ни одного подписанного пакета пока не установлено</div>
        <p class="muted small">Укажите путь к папке пакета (например, <code class="mono">dev-apps/doc-writer</code>) или нажмите кнопку быстрой установки выше.</p>
      </div>`;
    return;
  }

  for (const app of ctx.apps) {
    const card = document.createElement("div");
    card.className = "card" + (app.trusted ? "" : " untrusted");
    const allowOk = app.allowed !== false;
    const usable = app.trusted && allowOk;
    const allProblems = [...(app.problems || [])];
    if (!allowOk && app.allowlist_problem) allProblems.push(app.allowlist_problem);
    const problems = allProblems.length
      ? `<ul class="problems">${allProblems.map((p) => `<li>${esc(p)}</li>`).join("")}</ul>`
      : "";
    const allowBadge = allowOk
      ? ""
      : '<span class="badge bad">allowlist: запрещено</span>';

    const toolsCount = (app.tools || []).length;
    const caps = app.capabilities || {};
    const fileScope = (caps.file_scopes || []).includes("workspace");
    const externalWrites = (caps.external_writes || []).length > 0;
    const conns = (app.connections || [])
      .map((c) => `<span class="conn-tag">${esc(c.id)}</span>`)
      .join("");

    const history = app.history || [];
    const rollback = history.length
      ? `<button class="ghost small" data-rollback="${esc(app.app_id)}" data-version="${esc(history[0].version)}" title="Откатить к сохранённой версии">↩ v${esc(history[0].version)}</button>`
      : "";

    card.innerHTML = `
      <div class="app-card-top">
        <div class="app-identity">
          <div class="app-icon" aria-hidden="true">${esc(appInitials(app.name))}</div>
          <div class="app-copy">
            <div class="title">
              <span>${esc(app.name)}</span>
              <span class="version-pill">v${esc(app.version)}</span>
            </div>
            <div class="app-submeta">${esc(app.app_id)} · ${esc(app.publisher || "без подписанта")}</div>
          </div>
        </div>
        <span class="badge ${app.trusted ? "ok" : "bad"}">${app.trusted ? "подпись ок" : "не доверенный"}</span>
        ${allowBadge}
      </div>

      <div class="app-specs">
        <span class="spec-pill">Tools: <strong>${toolsCount}</strong></span>
        <span class="spec-pill">Файлы: <strong>${fileScope ? "workspace" : "нет"}</strong></span>
        <span class="spec-pill">Контур: <strong>${fileScope ? "локальный" : externalWrites ? "запись" : "read-only"}</strong></span>
      </div>

      ${problems}

      <div class="app-card-foot">
        <div class="conn-tags">${conns || '<span class="muted small">автономный</span>'}</div>
        <div class="row-actions">
          ${rollback}
          <button class="ghost small" data-remove="${esc(app.app_id)}">Удалить</button>
          <button class="small" data-open="${esc(app.app_id)}" ${usable ? "" : "disabled"}>Открыть среду</button>
        </div>
      </div>`;
    box.appendChild(card);
  }
}

async function installApp() {
  const path = $("install-path").value.trim();
  if (!path) return toast("Укажите путь к пакету", true);
  await guard(async () => {
    const app = await invoke("install_app", { path });
    toast(`Установлено: ${app.name} ${app.version}`);
    $("install-path").value = "";
    await refreshApps();
    await openApp(app.app_id);
  });
}

async function openApp(appId) {
  await guard(async () => {
    // Keep the previous app's engine alive: a running turn continues in the
    // background and streams into its own chat when the user switches back.
    const info = await invoke("open_app", { app_id: appId });
    ctx.appId = appId;
    ctx.appMeta = ctx.apps.find((a) => a.app_id === appId) || { tools: [], connections: [] };
    const hasMcp = (ctx.appMeta.connections || []).some((c) => c.type === "http-mcp");
    const toolsTab = document.querySelector('.ws-tab[data-ws="tools"]');
    if (toolsTab) toolsTab.hidden = !hasMcp;
    try {
      const saved = localStorage.getItem(`reasoning:${appId}`) || "";
      const efforts = [...$("chat-reasoning").options].map((option) => option.value);
      $("chat-reasoning").value = efforts.includes(saved) ? saved : "";
    } catch (_) {
      $("chat-reasoning").value = "";
    }
    ctx.chat = null;
    ctx.chatItems = [];
    ctx.chats = [];
    ctx.chatId = null;
    clearAttachments();
    setBusy(false);

    $("workspace").hidden = false;
    $("apps-panel").hidden = true;
    renderAppSwitcher();

    const wsIcon = $("ws-app-icon");
    if (wsIcon) wsIcon.textContent = appInitials(ctx.appMeta.name || appId);
    const wsVersion = $("ws-version");
    if (wsVersion) wsVersion.textContent = `v${ctx.appMeta.version || "0.1.0"}`;
    const wsPublisher = $("ws-publisher");
    if (wsPublisher) wsPublisher.textContent = `${appId} · ${ctx.appMeta.publisher || "Ed25519 verified"}`;
    const secTools = $("sec-tools-count");
    if (secTools) secTools.textContent = String((ctx.appMeta.tools || []).length);
    const caps = ctx.appMeta.capabilities || {};
    const fileScope = (caps.file_scopes || []).includes("workspace");
    const externalWrites = (caps.external_writes || []).length > 0;
    const secFiles = $("sec-file-scope");
    if (secFiles) secFiles.textContent = fileScope ? "workspace" : "DENY";
    const secWrites = $("sec-writes");
    if (secWrites) secWrites.textContent = externalWrites ? "РАЗРЕШЕНА" : "БЛОК";
    const wsToolsCount = $("ws-tools-count");
    if (wsToolsCount) wsToolsCount.textContent = String((ctx.appMeta.tools || []).length);

    document.querySelectorAll(".ws-tab").forEach((t) => t.classList.toggle("active", t.dataset.ws === "chat"));
    $("ws-chat").hidden = false;
    $("ws-tools").hidden = true;
    $("chat-log").innerHTML = "";
    renderChatList();
    renderChat([]);
    renderChatMeta();
    ensureChat().catch((e) => chatError(String(e)));

    $("ws-title").textContent = ctx.appMeta.name || appId;
    $("ws-title").hidden = true;
    const wsFolder = $("ws-folder");
    if (wsFolder) wsFolder.textContent = info.workspace || "—";

    if (hasMcp) renderToolSelectors();
  });
}

async function closeApp() {
  await guard(async () => {
    if (ctx.appId) {
      await invoke("chat_stop", { app_id: ctx.appId });
      await invoke("close_app", { app_id: ctx.appId });
    }
    await invoke("close_all_apps");
    ctx.appId = null;
    ctx.appMeta = null;
    $("apps-panel").hidden = false;
    $("app-switch").hidden = true;
    $("workspace").hidden = true;
  });
}

async function pickWorkspace() {
  if (!ctx.appId) return;
  let path;
  try {
    path = await invoke("choose_workspace", { app_id: ctx.appId });
  } catch (e) {
    return toast(String(e), true);
  }
  if (!path) return;
  const appId = ctx.appId;
  await closeApp();
  await openApp(appId);
  toast(`Рабочая папка: ${path}`);
}

async function removeApp(appId) {
  if (!confirm(`Удалить приложение ${appId}?`)) return;
  await guard(async () => {
    if (ctx.appId === appId) await closeApp();
    await invoke("remove_app", { app_id: appId });
    toast("Удалено");
    await refreshApps();
  });
}

async function rollbackApp(appId, version) {
  if (!confirm(`Откатить ${appId} к версии ${version}?`)) return;
  await guard(async () => {
    if (ctx.appId === appId) await closeApp();
    const app = await invoke("rollback_app", { app_id: appId, version });
    toast(`Откат: ${app.name} ${app.version}`);
    await refreshApps();
  });
}

/* ---------- chat ---------- */

function escapeHtml(value) {
  return String(value == null ? "" : value).replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[ch]));
}

function inlineMd(text) {
  return text
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>");
}

function renderMarkdown(source) {
  const lines = escapeHtml(source).split("\n");
  let html = "";
  let inCode = false;
  let list = null;
  const closeList = () => {
    if (list) {
      html += `</${list}>`;
      list = null;
    }
  };

  for (const raw of lines) {
    const line = raw.replace(/\s+$/, "");
    if (line.trimStart().startsWith("```")) {
      closeList();
      html += inCode ? "</code></pre>" : "<pre><code>";
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      html += line + "\n";
      continue;
    }
    if (!line.trim()) {
      closeList();
      continue;
    }
    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      closeList();
      const level = Math.min(heading[1].length + 2, 6);
      html += `<h${level}>${inlineMd(heading[2])}</h${level}>`;
      continue;
    }
    const bullet = line.match(/^\s*[-*]\s+(.*)$/);
    if (bullet) {
      if (list !== "ul") {
        closeList();
        html += "<ul>";
        list = "ul";
      }
      html += `<li>${inlineMd(bullet[1])}</li>`;
      continue;
    }
    const numbered = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (numbered) {
      if (list !== "ol") {
        closeList();
        html += "<ol>";
        list = "ol";
      }
      html += `<li>${inlineMd(numbered[1])}</li>`;
      continue;
    }
    closeList();
    html += `<p>${inlineMd(line)}</p>`;
  }
  closeList();
  if (inCode) html += "</code></pre>";
  return html;
}

function getStartersForApp(appId) {
  if (appId === "doc-writer") {
    return [
      "Создай в рабочей папке report.md с планом проверки гипотезы",
      "Прочитай report.md и сократи раздел «Вывод»",
      "Сконвертируй report.md в report.docx через docx_export",
    ];
  }
  if (appId === "doc-lookup") {
    return [
      "Найди документацию Tauri v2 по настройке capabilities и ACL",
      "Покажи примеры конфигурации Streamable HTTP MCP",
    ];
  }
  return [
    "Какие инструменты доступны в этом пакете?",
    "Помоги подготовить итоговый отчёт по сессии",
  ];
}

function chatItemKey(item) {
  return [
    item.role,
    item.kind,
    item.text || "",
    item.tokens_in ?? "",
    item.tokens_out ?? "",
    item.reasoning_tokens ?? "",
    item.ms ?? "",
    item.tok_s ?? "",
    item.model || "",
  ].join("\u0001");
}

function buildChatNode(item) {
  if (item.role === "tool") {
    const div = document.createElement("div");
    div.className = "msg tool";
    div.textContent = `⚙ tool: ${item.text}`;
    return div;
  }

  if (item.kind === "file") {
    const div = document.createElement("div");
    div.className = "msg " + (item.role === "user" ? "user" : "assistant");
    const chip = document.createElement("div");
    chip.className = "chat-attachment-chip";
    const kind = item.mime && item.mime.startsWith("image/") ? "изображение" : "файл";
    chip.textContent = `${kind}: ${item.text}`;
    div.appendChild(chip);
    return div;
  }

  if (item.kind === "reasoning") {
    const details = document.createElement("details");
    details.className = "msg reasoning";
    const summary = document.createElement("summary");
    summary.textContent = "Ход размышлений модели";
    const body = document.createElement("div");
    body.className = "msg-body";
    body.innerHTML = renderMarkdown(item.text);
    details.appendChild(summary);
    details.appendChild(body);
    return details;
  }

  const div = document.createElement("div");
  div.className = "msg " + (item.role === "user" ? "user" : "assistant");
  const body = document.createElement("div");
  body.className = "msg-body";
  if (item.role === "user") {
    body.textContent = item.text;
  } else {
    body.innerHTML = renderMarkdown(item.text);
  }
  div.appendChild(body);

  if (item.role === "assistant") {
    const footer = document.createElement("div");
    footer.className = "msg-foot";
    const stats = [];
    if (item.tokens_in != null) stats.push(`↑${fmtNum(item.tokens_in)}`);
    if (item.tokens_out != null) stats.push(`↓${fmtNum(item.tokens_out)}`);
    if (item.reasoning_tokens) stats.push(`reasoning ${fmtNum(item.reasoning_tokens)}`);
    if (item.tok_s) stats.push(`${Math.round(item.tok_s)} tok/s`);
    if (item.ms) stats.push(`${(item.ms / 1000).toFixed(1)} с`);
    if (item.model) stats.push(item.model);
    const meta = document.createElement("span");
    meta.className = "muted small mono";
    meta.textContent = stats.join(" · ");
    const copy = document.createElement("button");
    copy.className = "ghost small";
    copy.textContent = "Копировать";
    copy.dataset.copy = "1";
    copy.dataset.text = item.text;
    footer.appendChild(meta);
    footer.appendChild(copy);
    div.appendChild(footer);
  }

  return div;
}

// Incremental rendering: polling fires every 700 ms and most items do not
// change. Rebuilding the whole log made messages blink and reset the scroll.
function renderChat(items) {
  const log = $("chat-log");
  const rendered = ctx.rendered || (ctx.rendered = []);
  const atBottom = log.scrollHeight - log.scrollTop - log.clientHeight < 48;

  if (!items || !items.length) {
    if (log.querySelector(".chat-empty")) return;
    log.innerHTML = "";
    rendered.length = 0;
    const starters = getStartersForApp(ctx.appId)
      .map((s) => `<button type="button" class="starter-chip" data-starter="${esc(s)}">${esc(s)}</button>`)
      .join("");
    const empty = document.createElement("div");
    empty.className = "chat-empty";
    empty.innerHTML = `
      <div class="empty-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z" />
          <path d="M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z" />
        </svg>
      </div>
      <h3>Агент готов к работе</h3>
      <p>Сессия изолирована. Вызовы инструментов ограничены контрактом пакета и проходят через платформенный шлюз.</p>
      <div class="prompt-starters">${starters}</div>`;
    log.appendChild(empty);
    return;
  }

  if (log.querySelector(".chat-empty")) {
    log.innerHTML = "";
    rendered.length = 0;
  }

  for (let i = 0; i < items.length; i++) {
    const fingerprint = chatItemKey(items[i]);
    const entry = rendered[i];
    const current = log.children[i];
    if (entry && entry.fingerprint === fingerprint && entry.node === current) continue;

    const node = buildChatNode(items[i]);
    if (entry && entry.node && entry.node.tagName === "DETAILS" && entry.node.open) {
      node.open = true;
    }
    if (current) {
      log.replaceChild(node, current);
    } else {
      log.appendChild(node);
    }
    rendered[i] = { fingerprint, node };
  }

  while (rendered.length > items.length) {
    const entry = rendered.pop();
    if (entry.node.parentNode === log) log.removeChild(entry.node);
  }
  while (log.children.length > items.length) {
    log.removeChild(log.lastElementChild);
  }

  if (atBottom) log.scrollTop = log.scrollHeight;
}

function fmtNum(value) {
  if (value >= 1000000) return (value / 1000000).toFixed(1) + "M";
  if (value >= 1000) return (value / 1000).toFixed(1) + "k";
  return String(value);
}

function chatError(message) {
  const div = document.createElement("div");
  div.className = "msg error";
  div.textContent = message;
  $("chat-log").appendChild(div);
  $("chat-log").scrollTop = $("chat-log").scrollHeight;
}

let busyTimer = null;
let busyStart = 0;

function setBusy(on) {
  ctx.busy = Boolean(on);
  $("chat-busy").hidden = !on;
  $("chat-stop").hidden = !on;
  $("chat-send").disabled = Boolean(on);
  if (on) {
    busyStart = Date.now();
    $("chat-busy").textContent = "думает… 0 с";
    clearInterval(busyTimer);
    busyTimer = setInterval(() => {
      const secs = Math.round((Date.now() - busyStart) / 1000);
      $("chat-busy").textContent = `думает… ${secs} с`;
    }, 1000);
  } else {
    clearInterval(busyTimer);
    busyTimer = null;
    $("chat-busy").textContent = "думает…";
  }
}

function renderChatMeta(model) {
  const endpoint = (ctx.config && ctx.config.llm_endpoint) || "";
  const modelText = model || (ctx.chat && ctx.chat.model) || "движок не запущен";
  $("chat-model").textContent = `${modelText}${endpoint ? " · " + endpoint : ""}`;
  const local = /127\.0\.0\.1|localhost/i.test(endpoint);
  $("chat-warning").hidden = !local;
}

function renderAppSwitcher() {
  const sel = $("app-switch");
  if (!sel) return;
  sel.innerHTML = "";
  for (const app of ctx.apps || []) {
    const option = document.createElement("option");
    option.value = app.app_id;
    option.textContent = app.name;
    sel.appendChild(option);
  }
  if (ctx.appId) sel.value = ctx.appId;
  sel.hidden = !ctx.appId;
}

async function syncBusy() {
  if (!ctx.appId || !ctx.chatId) return setBusy(false);
  try {
    const status = await invoke("chat_status", { app_id: ctx.appId, session_id: ctx.chatId });
    setBusy(Boolean(status.busy));
  } catch (e) {
    setBusy(false);
  }
}

function renderChatList() {
  const box = $("chat-list");
  box.innerHTML = "";
  for (const chat of ctx.chats || []) {
    const row = document.createElement("div");
    row.className = "chat-row";

    const btn = document.createElement("button");
    btn.className = "chat-item" + (chat.id === ctx.chatId ? " active" : "");
    btn.textContent = chat.title;
    btn.title = chat.title;
    btn.dataset.chat = chat.id;

    const del = document.createElement("button");
    del.className = "chat-del";
    del.textContent = "×";
    del.title = "Удалить чат";
    del.dataset.del = chat.id;

    row.appendChild(btn);
    row.appendChild(del);
    box.appendChild(row);
  }
}

async function refreshChats() {
  try {
    ctx.chats = await invoke("chat_list", { app_id: ctx.appId });
  } catch (e) {
    ctx.chats = [];
  }
  renderChatList();
}

async function loadHistory() {
  const items = await invoke("chat_history", { app_id: ctx.appId });
  ctx.chatItems = items;
  ctx.acceptStream = false;
  ctx.streamingItems = [];
  $("chat-log").innerHTML = "";
  ctx.rendered = [];
  renderChat(items);
  await syncBusy();
}

async function ensureChat() {
  if (ctx.chat && ctx.chat.app === ctx.appId) return ctx.chat;
  $("chat-model").textContent = "запуск движка…";
  const info = await invoke("chat_open", { app_id: ctx.appId });
  ctx.chat = { app: ctx.appId, model: info.model };
  ctx.chatId = ctx.chatId || info.session;
  renderChatMeta(info.model);
  await refreshChats();

  if (!ctx.chatId) {
    const created = await invoke("chat_new", { app_id: ctx.appId });
    ctx.chatId = created.session;
    await refreshChats();
  } else if (!(ctx.chats || []).some((c) => c.id === ctx.chatId) && (ctx.chats || []).length) {
    ctx.chatId = ctx.chats[0].id;
    await invoke("chat_select", { app_id: ctx.appId, session_id: ctx.chatId });
  }
  await loadHistory();
  return ctx.chat;
}

async function newChat() {
  await guard(async () => {
    await ensureChat().catch(() => {});
    const info = await invoke("chat_new", { app_id: ctx.appId });
    ctx.chatId = info.session;
    ctx.chatItems = [];
    ctx.acceptStream = false;
    ctx.streamingItems = [];
    setBusy(false);
    renderChat([]);
    await refreshChats();
  });
}

async function selectChat(sessionId) {
  if (!sessionId || sessionId === ctx.chatId) return;
  await guard(async () => {
    await invoke("chat_select", { app_id: ctx.appId, session_id: sessionId });
    ctx.chatId = sessionId;
    setBusy(false);
    await loadHistory();
    renderChatList();
  });
}

async function deleteChat(sessionId) {
  if (!confirm("Удалить этот чат?")) return;
  await guard(async () => {
    const info = await invoke("chat_delete", { app_id: ctx.appId, session_id: sessionId });
    ctx.chatId = info.session;
    await refreshChats();
    await loadHistory();
  });
}

const MAX_ATTACHMENTS = 3;
const MAX_IMAGE_BYTES = 4 * 1024 * 1024;
const MAX_TEXT_BYTES = 64 * 1024;
const ATTACHMENT_TEXT_MIMES = new Set([
  "application/json",
  "application/xml",
  "application/x-yaml",
  "application/yaml",
  "application/toml",
  "application/javascript",
]);
const ATTACHMENT_EXT_MIME = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  gif: "image/gif",
  txt: "text/plain",
  md: "text/markdown",
  json: "application/json",
  csv: "text/csv",
  log: "text/plain",
  yaml: "application/x-yaml",
  yml: "application/x-yaml",
  xml: "application/xml",
  html: "text/html",
  css: "text/css",
  js: "application/javascript",
  ts: "text/plain",
  rs: "text/plain",
  py: "text/plain",
  toml: "application/toml",
  ini: "text/plain",
};

function attachmentMime(file) {
  const declared = String(file.type || "").toLowerCase();
  if (declared) return declared === "image/jpg" ? "image/jpeg" : declared;
  const ext = String(file.name || "").split(".").pop().toLowerCase();
  return ATTACHMENT_EXT_MIME[ext] || "";
}

function readFileBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result || "");
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error || new Error("не удалось прочитать файл"));
    reader.readAsDataURL(file);
  });
}

function renderAttachments() {
  const box = $("chat-attachments");
  if (!box) return;
  box.innerHTML = "";
  box.hidden = !ctx.attachments.length;
  ctx.attachments.forEach((attachment, index) => {
    const chip = document.createElement("div");
    chip.className = "chat-attachment-chip";
    const name = document.createElement("span");
    name.textContent = attachment.filename;
    const remove = document.createElement("button");
    remove.type = "button";
    remove.dataset.removeAttachment = String(index);
    remove.title = "Убрать вложение";
    remove.textContent = "×";
    chip.appendChild(name);
    chip.appendChild(remove);
    box.appendChild(chip);
  });
}

async function addAttachments(fileList) {
  const files = [...(fileList || [])];
  if (!files.length) return;
  const caps = (ctx.config && ctx.config.llm_capabilities) || {};
  for (const file of files) {
    if (ctx.attachments.length >= MAX_ATTACHMENTS) {
      toast(`Не больше ${MAX_ATTACHMENTS} вложений в сообщении`, true);
      break;
    }
    const mime = attachmentMime(file);
    const isImage = mime.startsWith("image/");
    const isText = mime.startsWith("text/") || ATTACHMENT_TEXT_MIMES.has(mime);
    if (!isImage && !isText) {
      toast(`Тип файла не поддерживается: ${file.name}`, true);
      continue;
    }
    if (isImage && caps.vision !== true) {
      toast("Модель не настроена на изображения: включите vision в настройках", true);
      continue;
    }
    if (isImage && file.size > MAX_IMAGE_BYTES) {
      toast(`Изображение больше ${Math.round(MAX_IMAGE_BYTES / (1024 * 1024))} МиБ`, true);
      continue;
    }
    if (isText && file.size > MAX_TEXT_BYTES) {
      toast(`Текстовый файл больше ${Math.round(MAX_TEXT_BYTES / 1024)} КиБ`, true);
      continue;
    }
    try {
      const data = await readFileBase64(file);
      ctx.attachments.push({ mime, filename: file.name || "attachment", data });
    } catch (e) {
      toast(String(e), true);
    }
  }
  renderAttachments();
}

function clearAttachments() {
  ctx.attachments = [];
  renderAttachments();
}

async function sendChat() {
  const text = $("chat-text").value.trim();
  const attachments = ctx.attachments.map((attachment) => ({
    mime: attachment.mime,
    filename: attachment.filename,
    data: attachment.data,
  }));
  if ((!text && !attachments.length) || ctx.busy) return;
  $("chat-text").value = "";
  clearAttachments();
  setBusy(true);
  const variant = $("chat-reasoning").value || null;
  const optimistic = [...(ctx.chatItems || [])];
  if (text) optimistic.push({ role: "user", kind: "text", text });
  for (const attachment of attachments) {
    optimistic.push({
      role: "user",
      kind: "file",
      text: attachment.filename,
      mime: attachment.mime,
    });
  }
  ctx.chatItems = optimistic;
  ctx.streamingItems = [];
  ctx.acceptStream = true;
  renderChat(optimistic);
  try {
    await ensureChat();
    await invoke("chat_send", { app_id: ctx.appId, text, variant, attachments });
    if (!(window.__TAURI__ && window.__TAURI__.event)) {
      ctx.acceptStream = false;
      await refreshChats();
      await loadHistory();
      setBusy(false);
    }
  } catch (e) {
    ctx.acceptStream = false;
    ctx.streamingItems = [];
    chatError(String(e));
    setBusy(false);
  }
}

async function stopChat() {
  await guard(async () => {
    await invoke("chat_abort", { app_id: ctx.appId });
    toast("Останавливаю текущий ход");
  });
}

function bindChatEvents() {
  const listen = window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen;
  if (!listen) return;
  listen("chat://stream", (event) => {
    const payload = event.payload || {};
    if (payload.app_id !== ctx.appId) return;
    if (payload.session && payload.session !== ctx.chatId) return;
    if (!ctx.acceptStream) return;
    ctx.streamingItems = payload.items || [];
    renderChat([...(ctx.chatItems || []), ...ctx.streamingItems]);
  });
  listen("chat://update", (event) => {
    const payload = event.payload || {};
    if (payload.app_id !== ctx.appId) return;
    if (payload.session && payload.session !== ctx.chatId) return;
    ctx.acceptStream = false;
    ctx.streamingItems = [];
    ctx.chatItems = payload.items || [];
    renderChat(ctx.chatItems);
  });
  listen("chat://busy", (event) => {
    const payload = event.payload || {};
    if (payload.app_id !== ctx.appId) return;
    if (payload.session && payload.session !== ctx.chatId) return;
    setBusy(payload.busy);
    if (!payload.busy) {
      ctx.acceptStream = false;
      ctx.streamingItems = [];
      refreshChats();
    }
  });
  listen("chat://error", (event) => {
    const payload = event.payload || {};
    if (payload.app_id !== ctx.appId) return;
    if (payload.session && payload.session !== ctx.chatId) return;
    ctx.acceptStream = false;
    ctx.streamingItems = [];
    chatError(payload.message || "ошибка движка");
    setBusy(false);
  });
}

/* ---------- mcp ---------- */

async function refreshMcpServers() {
  ctx.mcp = await invoke("mcp_servers");
  const box = $("mcp-servers");
  box.innerHTML = ctx.mcp.length ? "" : '<span class="muted small">зарегистрированных MCP-серверов нет</span>';
  for (const id of ctx.mcp) {
    const row = document.createElement("div");
    row.className = "secret-row";
    row.innerHTML = `<span>${esc(id)}</span><div class="row-actions">
      <button class="ghost small" data-mcp-probe="${esc(id)}">Проверить tools</button>
      <button class="ghost small danger-hover" data-mcp-del="${esc(id)}">Удалить</button></div>`;
    box.appendChild(row);
  }
  renderToolSelectors();
}

function renderToolSelectors() {
  const toolSel = $("tool-select");
  const serverSel = $("server-select");
  const manifestBox = $("manifest-tools-list");
  if (!toolSel || !serverSel) return;
  toolSel.innerHTML = "";
  serverSel.innerHTML = "";
  if (manifestBox) manifestBox.innerHTML = "";

  const tools = (ctx.appMeta && ctx.appMeta.tools) || [];
  for (const t of tools) {
    const o = document.createElement("option");
    o.value = t.tool;
    o.textContent = t.tool;
    toolSel.appendChild(o);

    if (manifestBox) {
      const item = document.createElement("div");
      item.className = "manifest-tool-item";
      item.dataset.pickTool = t.tool;
      const stream = t.stream || "read";
      item.innerHTML = `
        <span class="manifest-tool-name">${esc(t.tool)}</span>
        <span class="stream-tag ${stream === "write" ? "write" : "read"}">${esc(stream)}</span>
      `;
      manifestBox.appendChild(item);
    }
  }

  const conns = ((ctx.appMeta && ctx.appMeta.connections) || [])
    .filter((c) => c.type === "http-mcp")
    .map((c) => c.id);
  const servers = Array.from(new Set([...(ctx.mcp || []), ...conns]));
  for (const s of servers) {
    const o = document.createElement("option");
    o.value = s;
    o.textContent = s;
    serverSel.appendChild(o);
  }
  const preferred = conns.find((c) => (ctx.mcp || []).includes(c));
  if (preferred) serverSel.value = preferred;

  const ready = tools.length > 0 && servers.length > 0;
  $("tool-call").disabled = !ready;
}

async function saveMcpServer() {
  const id = $("mcp-id").value.trim();
  const url = $("mcp-url").value.trim();
  if (!id || !url) return toast("ID и URL обязательны", true);
  await guard(async () => {
    await invoke("mcp_server_save", { id, url, token: $("mcp-token").value || null });
    $("mcp-id").value = "";
    $("mcp-url").value = "";
    $("mcp-token").value = "";
    toast("MCP-сервер сохранён в DPAPI");
    await refreshMcpServers();
  });
}

async function probeMcp(id) {
  await guard(async () => {
    const r = await invoke("mcp_probe", { id });
    const tools = (r.tools || []).join(", ");
    toast(`${(r.server && r.server.name) || id}: ${tools || "инструменты не найдены"}`);
  });
}

async function callTool() {
  const tool = $("tool-select").value;
  const serverId = $("server-select").value;
  if (!tool || !serverId) return toast("Выберите сервер и инструмент", true);
  let args;
  try {
    args = $("tool-args").value.trim() ? JSON.parse($("tool-args").value) : {};
  } catch (e) {
    return toast("Аргументы должны быть JSON: " + e.message, true);
  }
  await guard(async () => {
    $("tool-meta").textContent = "выполняется запрос…";
    const r = await invoke("mcp_call", {
      app_id: ctx.appId,
      server_id: serverId,
      tool,
      arguments: args,
    });
    const pre = $("tool-result");
    const empty = $("tool-empty-result");
    if (empty) empty.hidden = true;
    pre.hidden = false;
    pre.textContent = r.result;
    $("tool-meta").textContent = `${r.bytes} байт${r.truncated ? " · усечено" : ""}`;
  });
}

/* ---------- artifacts ---------- */

function fmtBytes(value) {
  const bytes = Number(value) || 0;
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} ГиБ`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} МиБ`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} КиБ`;
  return `${bytes} Б`;
}

function renderArtifactApps() {
  const select = $("artifact-app");
  if (!select) return;
  const current = select.value;
  select.innerHTML = "";
  for (const app of ctx.apps || []) {
    const option = document.createElement("option");
    option.value = app.app_id;
    option.textContent = `${app.name} (${app.app_id})`;
    select.appendChild(option);
  }
  if (current && (ctx.apps || []).some((app) => app.app_id === current)) {
    select.value = current;
  }
  select.disabled = !select.options.length;
}

let artifactRefreshToken = 0;

async function refreshArtifacts() {
  const appId = $("artifact-app").value;
  const sessionSelect = $("artifact-session");
  const dumpsBox = $("artifact-dumps");
  $("artifact-results").innerHTML = "";
  if (!appId) {
    $("artifact-usage").textContent = "нет установленных приложений";
    sessionSelect.innerHTML = '<option value="">все сессии</option>';
    dumpsBox.innerHTML = "";
    return;
  }
  const token = ++artifactRefreshToken;
  const [usage, dumps] = await Promise.all([
    invoke("artifacts_usage", { app_id: appId }),
    invoke("artifact_dumps", { app_id: appId }),
  ]);
  if (token !== artifactRefreshToken || $("artifact-app").value !== appId) return;
  $("artifact-usage").textContent = `${usage.sessions} сессий · ${fmtBytes(usage.bytes)} · ${usage.files} файлов`;

  const selected = sessionSelect.value;
  sessionSelect.innerHTML = '<option value="">все сессии</option>';
  for (const dump of dumps) {
    const option = document.createElement("option");
    option.value = dump.session;
    option.textContent = `${dump.session} · ${fmtBytes(dump.bytes)}`;
    sessionSelect.appendChild(option);
  }
  if (selected && dumps.some((dump) => dump.session === selected)) {
    sessionSelect.value = selected;
  }

  dumpsBox.innerHTML = dumps.length ? "" : '<span class="muted small">дампов пока нет</span>';
  for (const dump of dumps) {
    const row = document.createElement("div");
    row.className = "artifact-row";
    const when = dump.modified ? new Date(dump.modified * 1000).toLocaleString() : "";
    row.innerHTML = `
      <span class="mono">${esc(dump.session)}</span>
      <code>${fmtBytes(dump.bytes)}${when ? ` · ${esc(when)}` : ""}</code>`;
    dumpsBox.appendChild(row);
  }
}

async function searchArtifacts() {
  const appId = $("artifact-app").value;
  const query = $("artifact-query").value.trim();
  if (!appId) return toast("Нет установленных приложений", true);
  if (!query) return toast("Введите строку поиска", true);
  await guard(async () => {
    const session = $("artifact-session").value || null;
    const limit = 50;
    const hits = await invoke("artifact_grep", {
      app_id: appId,
      query,
      session,
      limit,
    });
    const box = $("artifact-results");
    box.innerHTML = hits.length ? "" : '<span class="muted small">совпадений нет</span>';
    for (const hit of hits) {
      const row = document.createElement("div");
      row.className = "artifact-row";
      row.innerHTML = `
        <span class="mono">${esc(hit.session)}:${hit.line}</span>
        <code title="${esc(hit.text)}">${esc(hit.text)}</code>`;
      box.appendChild(row);
    }
    if (!hits.length) toast("Совпадений нет");
    else if (hits.length >= limit) toast(`Показано ${hits.length} совпадений (лимит ${limit}), возможно есть ещё`);
    else toast(`Найдено совпадений: ${hits.length}`);
  });
}

async function exportArtifacts() {
  const appId = $("artifact-app").value;
  if (!appId) return toast("Нет установленных приложений", true);
  await guard(async () => {
    const session = $("artifact-session").value || null;
    const report = await invoke("export_artifacts", { app_id: appId, session });
    if (!report) return;
    if (!report.files) return toast("Экспортировать нечего: дампов нет", true);
    toast(`Экспортировано ${report.files} файл(ов), ${fmtBytes(report.bytes)} → ${report.dir}`);
  });
}

async function cleanupArtifacts() {
  const appId = $("artifact-app").value;
  if (!appId) return toast("Нет установленных приложений", true);
  await guard(async () => {
    const report = await invoke("cleanup_artifacts", { app_id: appId });
    toast(`Удалено сессий: ${report.removed} · ${fmtBytes(report.bytes_freed)}`);
    await refreshArtifacts();
  });
}

/* ---------- settings & secrets ---------- */

const EFFORT_LABELS = {
  low: "Низкий (low)",
  medium: "Средний (medium)",
  high: "Глубокий (high)",
  max: "Максимальный (max)",
};

function renderReasoningOptions() {
  const select = $("chat-reasoning");
  const wrap = $("chat-reasoning-wrap");
  if (!select) return;
  const caps = (ctx.config && ctx.config.llm_capabilities) || {};
  const efforts =
    Array.isArray(caps.reasoning_efforts) && caps.reasoning_efforts.length
      ? caps.reasoning_efforts
      : ["low", "medium", "high"];
  const enabled = caps.reasoning !== false;
  if (wrap) wrap.hidden = !enabled;
  select.disabled = !enabled;
  select.innerHTML = '<option value="">Авто</option>';
  if (enabled) {
    for (const effort of efforts) {
      const option = document.createElement("option");
      option.value = effort;
      option.textContent = EFFORT_LABELS[effort] || effort;
      select.appendChild(option);
    }
  }
  select.value = "";
}

async function loadConfig() {
  ctx.config = await invoke("get_config");
  $("llm-endpoint").value = ctx.config.llm_endpoint || "";
  $("llm-model").value = ctx.config.llm_model || "";
  const caps = ctx.config.llm_capabilities || {};
  $("llm-vision").checked = !!caps.vision;
  $("llm-reasoning").checked = caps.reasoning !== false;
  $("llm-efforts").value = (
    Array.isArray(caps.reasoning_efforts) && caps.reasoning_efforts.length
      ? caps.reasoning_efforts
      : ["low", "medium", "high"]
  ).join(",");
  $("llm-context").value = caps.context_tokens || "";
  $("llm-output").value = caps.output_tokens || "";
  $("q-session").value = ctx.config.session_quota_mb;
  $("q-global").value = ctx.config.global_quota_mb;
  $("q-ttl").value = ctx.config.ttl_hours;
  renderReasoningOptions();
}

async function saveConfig() {
  await guard(async () => {
    const efforts = $("llm-efforts")
      .value.split(",")
      .map((effort) => effort.trim().toLowerCase())
      .filter(Boolean);
    ctx.config = {
      ...(ctx.config || {}),
      llm_endpoint: $("llm-endpoint").value.trim() || null,
      llm_model: $("llm-model").value.trim() || null,
      llm_capabilities: {
        vision: $("llm-vision").checked,
        reasoning: $("llm-reasoning").checked,
        reasoning_efforts: efforts.length ? [...new Set(efforts)] : ["low", "medium", "high"],
        context_tokens: Number($("llm-context").value) || null,
        output_tokens: Number($("llm-output").value) || null,
      },
      session_quota_mb: Number($("q-session").value) || 500,
      global_quota_mb: Number($("q-global").value) || 5120,
      ttl_hours: Number($("q-ttl").value) || 168,
    };
    await invoke("save_config", { config: ctx.config });
    ctx.chat = null;
    ctx.chatItems = [];
    setBusy(false);
    renderChat([]);
    renderChatMeta();
    renderReasoningOptions();
    toast("Настройки сохранены, движок перезапустится при следующем сообщении");
  });
}

const LLM_KEY_NAME = "llm_api_key";

async function refreshLlmKeyStatus() {
  const names = await invoke("secrets_list");
  const has = names.includes(LLM_KEY_NAME);
  const statusEl = $("llm-key-status");
  statusEl.textContent = has ? "ключ в DPAPI" : "ключ не задан";
  statusEl.className = "badge mono " + (has ? "ok" : "bad");
  $("llm-key").placeholder = has ? "•••••••• (сохранён в DPAPI)" : "ключ не задан";
}

async function saveLlmKey() {
  const value = $("llm-key").value;
  if (!value) return toast("Введите ключ", true);
  await guard(async () => {
    await invoke("secret_store", { name: LLM_KEY_NAME, value });
    $("llm-key").value = "";
    toast("Ключ модели сохранён в DPAPI");
    await refreshLlmKeyStatus();
    await refreshSecrets();
  });
}

async function deleteLlmKey() {
  await guard(async () => {
    await invoke("secret_delete", { name: LLM_KEY_NAME });
    toast("Ключ модели удалён");
    await refreshLlmKeyStatus();
    await refreshSecrets();
  });
}

async function probe() {
  const url = $("llm-endpoint").value.trim();
  if (!url) return toast("Укажите endpoint", true);
  await guard(async () => {
    const r = await invoke("probe_endpoint", { url });
    $("probe-out").textContent = `${r.ok ? "✓ OK" : "✕ FAIL"} · ${r.millis} мс · ${r.detail}`;
  });
}

async function refreshSecrets() {
  await guard(async () => {
    const names = await invoke("secrets_list");
    const box = $("secrets");
    box.innerHTML = names.length ? "" : '<span class="muted small">сохранённых секретов нет</span>';
    for (const name of names) {
      const row = document.createElement("div");
      row.className = "secret-row";
      row.innerHTML = `<span>${esc(name)}</span><button class="ghost small danger-hover" data-secret="${esc(name)}">удалить</button>`;
      box.appendChild(row);
    }
  });
}

async function saveSecret() {
  const name = $("secret-name").value.trim();
  const value = $("secret-value").value;
  if (!name || !value) return toast("Имя и значение обязательны", true);
  await guard(async () => {
    await invoke("secret_store", { name, value });
    $("secret-name").value = "";
    $("secret-value").value = "";
    toast("Секрет сохранён в DPAPI");
    await refreshSecrets();
  });
}

async function deleteSecret(name) {
  await guard(async () => {
    await invoke("secret_delete", { name });
    await refreshSecrets();
    await refreshLlmKeyStatus();
  });
}

/* ---------- wiring ---------- */

function bind() {
  document.querySelectorAll(".tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t === tab));
      document
        .querySelectorAll(".tab-body")
        .forEach((b) => b.classList.toggle("active", b.id === `tab-${tab.dataset.tab}`));
      if (tab.dataset.tab === "settings") guard(refreshArtifacts);
    });
  });

  $("install-btn").addEventListener("click", installApp);

  document.querySelectorAll("[data-quick-install]").forEach((btn) => {
    btn.addEventListener("click", () => {
      $("install-path").value = btn.dataset.quickInstall;
      installApp();
    });
  });

  $("apps").addEventListener("click", (e) => {
    const open = e.target.closest("[data-open]");
    if (open) return openApp(open.dataset.open);
    const remove = e.target.closest("[data-remove]");
    if (remove) return removeApp(remove.dataset.remove);
    const rollback = e.target.closest("[data-rollback]");
    if (rollback) return rollbackApp(rollback.dataset.rollback, rollback.dataset.version);
  });

  const wsBack = $("ws-back");
  if (wsBack) wsBack.addEventListener("click", closeApp);
  $("ws-close").addEventListener("click", closeApp);

  $("app-switch").addEventListener("change", (e) => {
    if (e.target.value && e.target.value !== ctx.appId) openApp(e.target.value);
  });
  $("ws-folder-pick").addEventListener("click", pickWorkspace);

  $("settings-save").addEventListener("click", saveConfig);
  $("artifact-refresh").addEventListener("click", () => guard(refreshArtifacts));
  $("artifact-search").addEventListener("click", searchArtifacts);
  $("artifact-export").addEventListener("click", exportArtifacts);
  $("artifact-cleanup").addEventListener("click", cleanupArtifacts);
  $("artifact-app").addEventListener("change", () => guard(refreshArtifacts));
  $("artifact-session").addEventListener("change", () => {
    $("artifact-results").innerHTML = "";
  });
  $("artifact-query").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      searchArtifacts();
    }
  });
  $("probe-btn").addEventListener("click", probe);
  $("secret-save").addEventListener("click", saveSecret);
  $("secret-refresh").addEventListener("click", refreshSecrets);
  $("llm-key-save").addEventListener("click", saveLlmKey);
  $("llm-key-delete").addEventListener("click", deleteLlmKey);
  $("mcp-save").addEventListener("click", saveMcpServer);
  $("mcp-refresh").addEventListener("click", () => guard(refreshMcpServers));
  $("tool-call").addEventListener("click", callTool);

  const sampleBtn = $("tool-sample-btn");
  if (sampleBtn) {
    sampleBtn.addEventListener("click", () => {
      const tool = $("tool-select").value;
      const samples = {
        "resolve-library-id": { libraryName: "tauri", query: "capabilities permissions" },
        "query-docs": { libraryId: "/tauri-apps/tauri-docs", query: "command permissions allowlist" },
      };
      $("tool-args").value = JSON.stringify(samples[tool] || { query: "test" }, null, 2);
    });
  }

  const manifestList = $("manifest-tools-list");
  if (manifestList) {
    manifestList.addEventListener("click", (e) => {
      const item = e.target.closest("[data-pick-tool]");
      if (item && $("tool-select")) {
        $("tool-select").value = item.dataset.pickTool;
      }
    });
  }

  $("chat-send").addEventListener("click", sendChat);
  $("chat-new").addEventListener("click", newChat);
  $("chat-stop").addEventListener("click", stopChat);
  $("chat-list").addEventListener("click", (e) => {
    const del = e.target.closest("[data-del]");
    if (del) {
      e.stopPropagation();
      deleteChat(del.dataset.del);
      return;
    }
    const item = e.target.closest("[data-chat]");
    if (item) selectChat(item.dataset.chat);
  });
  $("chat-log").addEventListener("click", (e) => {
    const starter = e.target.closest("[data-starter]");
    if (starter) {
      $("chat-text").value = starter.dataset.starter || "";
      $("chat-text").focus();
      return;
    }
    const copy = e.target.closest("[data-copy]");
    if (!copy) return;
    navigator.clipboard.writeText(copy.dataset.text || "").then(
      () => toast("Скопировано в буфер обмена"),
      () => toast("Не удалось скопировать", true)
    );
  });
  $("chat-reasoning").addEventListener("change", (e) => {
    try {
      localStorage.setItem(`reasoning:${ctx.appId}`, e.target.value);
    } catch (_) {}
    toast(
      e.target.value
        ? `Уровень размышлений: ${e.target.selectedOptions[0].textContent}`
        : "Уровень размышлений: авто"
    );
  });
  $("chat-text").addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendChat();
    }
  });

  $("chat-file-input").addEventListener("change", async (event) => {
    await addAttachments(event.target.files);
    event.target.value = "";
  });

  $("chat-attachments").addEventListener("click", (event) => {
    const remove = event.target.closest("[data-remove-attachment]");
    if (!remove) return;
    const index = Number(remove.dataset.removeAttachment);
    if (Number.isInteger(index)) {
      ctx.attachments.splice(index, 1);
      renderAttachments();
    }
  });

  const composer = document.querySelector(".chat-composer-wrap");
  if (composer) {
    composer.addEventListener("dragover", (event) => {
      event.preventDefault();
      composer.classList.add("drop-active");
    });
    composer.addEventListener("dragleave", () => composer.classList.remove("drop-active"));
    composer.addEventListener("drop", (event) => {
      event.preventDefault();
      composer.classList.remove("drop-active");
      addAttachments(event.dataTransfer.files);
    });
  }

  document.querySelectorAll(".ws-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      document.querySelectorAll(".ws-tab").forEach((t) => t.classList.toggle("active", t === tab));
      for (const name of ["chat", "tools"]) {
        const body = $(`ws-${name}`);
        if (body) body.hidden = name !== tab.dataset.ws;
      }
    });
  });

  $("mcp-servers").addEventListener("click", (e) => {
    const probeBtn = e.target.closest("[data-mcp-probe]");
    if (probeBtn) return probeMcp(probeBtn.dataset.mcpProbe);
    const del = e.target.closest("[data-mcp-del]");
    if (del) {
      return guard(async () => {
        await invoke("mcp_server_delete", { id: del.dataset.mcpDel });
        await refreshMcpServers();
      });
    }
  });

  $("secrets").addEventListener("click", (e) => {
    const del = e.target.closest("[data-secret]");
    if (del) deleteSecret(del.dataset.secret);
  });
}

async function init() {
  initTheme();
  bind();
  bindChatEvents();
  await guard(async () => {
    const info = await invoke("platform_info");
    $("home").textContent = info.home;
    $("version").textContent = `v${info.version} · schema ${info.schema}`;
    const trust = $("trust");
    const anchorSource = info.anchor_source === "machine" ? "machine" : info.anchor_source === "dev" ? "dev" : null;
    const anchorCount = info.anchor_count || 0;
    trust.textContent = info.trusted_anchor
      ? `trust anchor: ${info.anchor_fingerprint || "установлен"}${anchorCount > 1 ? ` +${anchorCount - 1}` : ""}${anchorSource ? ` (${anchorSource})` : ""}`
      : "trust anchor: НЕТ";
    trust.className = "badge " + (info.trusted_anchor ? (anchorSource === "dev" ? "warn" : "ok") : "bad");
    const allow = $("allowlist");
    const allowStatus = info.allowlist_status || "absent";
    const allowSource = info.allowlist_source && info.allowlist_source !== "none" ? ` (${info.allowlist_source})` : "";
    if (allowStatus === "valid") {
      allow.textContent = `allowlist: ${info.allowlist_apps || 0}${allowSource}`;
      allow.className = "badge ok";
    } else if (allowStatus === "expired" || allowStatus === "invalid" || allowStatus === "incomplete") {
      const label = allowStatus === "expired" ? "истёк" : allowStatus === "incomplete" ? "не доставлен" : "ошибка";
      allow.textContent = `allowlist: ${label}`;
      allow.className = "badge bad";
      allow.title = info.allowlist_problem || "Подписанный allowlist не проходит проверку";
    } else {
      allow.textContent = "allowlist: нет";
      allow.className = "badge";
      allow.title = "Подписанный allowlist не настроен: действует только проверка подписи пакета";
    }
    await loadConfig();
    await refreshLlmKeyStatus();
    await refreshApps();
    await refreshArtifacts();
    await refreshSecrets();
    await refreshMcpServers();
  });
}

window.addEventListener("DOMContentLoaded", init);
