const browserMock = (() => {
  const mockApps = [
    {
      app_id: "doc-writer",
      name: "Документы и Word",
      version: "0.1.0",
      publisher: "CN=ECO AIOps Dev",
      description: "Готовит документы, правит markdown и конвертирует их в Word.",
      icon: "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA4IDgiPjxyZWN0IHdpZHRoPSI4IiBoZWlnaHQ9IjgiIHJ4PSIyIiBmaWxsPSIjMjU2M2ViIi8+PC9zdmc+",
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
      version: "0.4.1",
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
            text: "### Готово\n\nСоздал в рабочей папке:\n\n- `report.md` — план проверки\n- `report.docx` — Word-версия через `docx_export`\n\n| Файл | Что внутри | Размер |\n| --- | --- | --- |\n| `report.md` | план проверки | 2.1 КБ |\n| `report.docx` | Word-версия | 14.8 КБ |\n\n```sql\nSELECT pid, mode FROM pg_locks WHERE NOT granted;\n```\n\nФайлы можно править и пересобирать.\n\n```mermaid\nflowchart LR\n  A[report.md] --> B[docx_export]\n  B --> C[report.docx]\n```\n\n```chart\n{\"type\":\"bar\",\"labels\":[\"read\",\"write\"],\"datasets\":[{\"label\":\"files\",\"data\":[2,1]}]}\n```\n\n```mermaid\nxychart-beta\n    title \"Sales\"\n    x-axis [янв, фев, мар]\n    y-axis \"k\" 0 --> 250\n    bar [120, 170, 100]\n```",
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
  let packManifest = false;
  let permissionGrants = [
    {
      id: "pg_demo",
      app_id: "doc-writer",
      action: "platform_write_file",
      tool: "write_file",
      stream: "write",
      app_version: "0.1.0",
      workspace: "D:\\work\\doc-writer",
      mode: "corporate",
      bin: null,
      bin_sha256: null,
      created: 1700000000,
      expires: null,
    },
  ];
  let config = {
    llm_endpoint: "https://llm-gateway.internal/v1",
    llm_model: "~deepseek/deepseek-v4-flash-latest",
    llm_allow_invalid_certs: false,
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
    environment: "corporate",
    role: "developer",
    mode_chosen: true,
  };

  return async function invokeMock(cmd, args = {}) {
    switch (cmd) {
      case "platform_info":
        return {
          home: "D:\\git\\rust-ai-app\\.data",
          version: "0.5.0",
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
          environment: config.environment,
          role: config.role,
          mode_chosen: config.mode_chosen,
          mode_locked: false,
          allow_authoring: config.role === "developer",
        };
      case "set_mode":
        config = {
          ...config,
          environment: args.environment,
          role: args.role,
          mode_chosen: true,
        };
        return config;
      case "set_role":
        config = { ...config, role: args.role };
        return config;
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
      case "mcp_server_get":
        return {
          id: args.id,
          url: "https://mcp.example.internal/…",
          has_token: true,
          allow_invalid_certs: false,
          scope: "global",
          has_app_override: false,
        };
      case "mcp_server_save":
        if (args.id && !mcpServers.includes(args.id)) mcpServers.push(args.id);
        return null;
      case "mcp_server_delete":
        mcpServers = mcpServers.filter((s) => s !== args.id);
        return null;
      case "mcp_probe":
      case "mcp_app_probe":
        return {
          protocol: "2025-06-18",
          server: { name: args.id },
          tools: ["resolve-library-id", "query-docs"],
        };
      case "mcp_app_servers": {
        const app = mockApps.find((a) => a.app_id === args.app_id);
        return ((app && app.connections) || [])
          .filter((c) => c.type === "http-mcp")
          .map((c) => ({
            id: c.id,
            url: "https://mcp.example.internal/…",
            has_token: true,
            allow_invalid_certs: false,
            scope: "global",
            has_app_override: false,
          }));
      }
      case "mcp_app_server_save":
      case "mcp_app_server_delete":
        return null;
      case "save_diagram":
        return `${config.workspaces?.[args.app_id] || "workspace"}/${args.filename}`;
      case "app_secrets":
        return [];
      case "app_secret_set":
      case "app_secret_delete":
        return null;
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
        return { ok: true, millis: 38, detail: "TCP connect ok (llm-gateway.internal:443)" };
      case "open_app":
        return {
          app_id: args.app_id,
          workspace: `D:\\work\\${args.app_id}`,
        };
      case "choose_workspace":
        return `D:\\work\\${args.app_id}`;
      case "workspace_file_open":
      case "workspace_file_reveal":
      case "workspace_open_folder":
        return null;
      case "workspace_file_save_copy":
        return `D:\\Downloads\\${String(args.path || "artifact").split("/").pop()}`;
      case "open_external":
        return null;
      case "permission_grants":
        return permissionGrants.filter(
          (grant) => !args.app_id || grant.app_id === args.app_id,
        );
      case "permission_revoke_grant":
        permissionGrants = permissionGrants.filter((grant) => grant.id !== args.id);
        return null;
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
      case "pack_tool_info":
        return {
          available: true,
          path: "D:\\git\\rust-ai-app\\vendor\\pack\\eco-pack.exe",
          version: "eco-pack 0.1.0",
          default_key: "D:\\git\\rust-ai-app\\.data\\dev\\keys\\dev.key",
        };
      case "pack_files":
        return [
          { path: "manifest.json", bytes: 640 },
          { path: "prompt/system.md", bytes: 220 },
          { path: "skills/main/SKILL.md", bytes: 180 },
        ];
      case "pack_scan":
        return {
          has_manifest: packManifest,
          prompt: packManifest ? "prompt/system.md" : null,
          agents_md: true,
          root_skill: true,
          skills: [],
          bins: [],
          mcp: [{ id: "context7", source: ".mcp.json" }],
          env_files: [],
        };
      case "pack_import":
        packManifest = true;
        return {
          ok: true,
          output: "AGENTS.md -> prompt/system.md\nSKILL.md -> skills/main/SKILL.md\nmanifest.json created",
        };
      case "pack_apply_template":
        return {
          ok: true,
          output: `file_scopes: ${args.template === "files" ? "workspace" : "none"}\nmanifest updated; rebuild and re-sign the package`,
        };
      case "pack_set_mcp":
        return {
          ok: true,
          output: `MCP '${args.id}': http-mcp, запись ${args.write ? "разрешена" : "выключена"}\nURL и токен задаются в Настройках`,
        };
      case "pack_read_text":
        if (args.path === "manifest.json") {
          return JSON.stringify(
            {
              schema_version: 2,
              app_id: "demo-app",
              name: "Demo App",
              version: "0.1.0",
              connections: [
                { id: "llm", type: "llm", required: true },
                { id: "context7", type: "http-mcp", required: false },
              ],
              capabilities: { file_scopes: ["workspace"], external_writes: [] },
            },
            null,
            2
          );
        }
        return `# ${args.path}\n\nMock file content.\n`;
      case "pack_write_text":
      case "pack_pick_folder":
        return args.title && args.title.includes("ключ") ? "D:\\keys\\me" : "D:\\apps\\demo-app";
      case "pack_init":
        packManifest = true;
        return { ok: true, output: "created package skeleton" };
      case "pack_validate":
      case "pack_build":
      case "pack_sign":
      case "pack_keygen":
        return { ok: true, output: `eco-pack ${args.__cmd || "ok"}` };
      case "install_app":
        return { app_id: args.path ? "demo-app" : "unknown", name: "Demo App", version: "0.1.0" };
      case "pick_package":
        return "dev-apps/doc-writer";
      case "chat_open":
        return {
          model: config.llm_model,
          session: activeSession[args.app_id] || "ses-101",
        };
      case "chat_list":
        return (mockChats[args.app_id] || []).map((c, index) => ({
          id: c.id,
          title: c.title,
          updated: Date.now() - index * 86400000,
        }));
      case "chat_rename": {
        const sessions = mockChats[args.app_id] || [];
        const chat = sessions.find((c) => c.id === args.session_id);
        if (chat) chat.title = args.title;
        return null;
      }
      case "chat_history": {
        const sessions = mockChats[args.app_id] || [];
        const current = sessions.find((s) => s.id === activeSession[args.app_id]) || sessions[0];
        return current ? current.items : [];
      }
      case "chat_image":
        return {
          mime: "image/png",
          data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        };
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
  pendingPermissions: [],
  chatStart: null,
  reasoning: "",
  compacting: false,
  memory: null,
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
  const log = $("chat-log");
  if (log && ctx.chatItems && ctx.chatItems.length) {
    destroyCharts(log);
    renderChat([...(ctx.chatItems || []), ...(ctx.streamingItems || [])]);
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

function toast(message, isError) {
  const stack = $("toast");
  if (!stack) return;
  const el = document.createElement("div");
  el.className = "toast" + (isError ? " error" : "");
  el.textContent = message;
  el.title = "Нажмите, чтобы скрыть";
  el.addEventListener("click", () => el.remove());
  stack.appendChild(el);
  while (stack.children.length > 4) stack.firstElementChild.remove();
  setTimeout(() => {
    el.classList.add("leaving");
    setTimeout(() => el.remove(), 240);
  }, isError ? 8000 : 3400);
}

/* ---------- confirmation dialog ---------- */

let confirmResolve = null;

/** Replaces window.confirm: resolves true on confirm, false on cancel/Escape. */
function askConfirm(options) {
  const opts = typeof options === "string" ? { message: options } : options || {};
  return new Promise((resolve) => {
    confirmResolve = resolve;
    $("confirm-title").textContent = opts.title || "Подтвердить действие?";
    $("confirm-message").textContent = opts.message || "";
    const ok = $("confirm-ok");
    ok.textContent = opts.confirmLabel || "Подтвердить";
    ok.classList.toggle("danger", Boolean(opts.danger));
    $("confirm-overlay").hidden = false;
    ok.focus();
  });
}

function settleConfirm(value) {
  const overlay = $("confirm-overlay");
  if (!overlay || overlay.hidden) return;
  overlay.hidden = true;
  const resolve = confirmResolve;
  confirmResolve = null;
  if (resolve) resolve(value);
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

    const icon = app.icon
      ? `<img class="app-icon-img" src="${esc(app.icon)}" alt="" loading="lazy">`
      : esc(appInitials(app.name));
    const description = app.description
      ? `<div class="app-desc">${esc(app.description)}</div>`
      : "";

    card.innerHTML = `
      <div class="app-card-top">
        <div class="app-identity">
          <div class="app-icon" aria-hidden="true">${icon}</div>
          <div class="app-copy">
            <div class="title">
              <span>${esc(app.name)}</span>
              <span class="version-pill">v${esc(app.version)}</span>
            </div>
            <div class="app-submeta">${esc(app.app_id)} · ${esc(app.publisher || "без подписанта")}</div>
            ${description}
          </div>
        </div>
        <span class="badge ${app.trusted ? "ok" : "bad"}">${app.trusted ? (app.signer_fingerprint ? "подпись ок" : "целостность ок") : "не доверенный"}</span>
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

async function pickPackage(kind) {
  await guard(async () => {
    const path = await invoke("pick_package", { kind });
    if (path) $("install-path").value = path;
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
      ctx.reasoning = localStorage.getItem(`reasoning:${appId}`) || "";
    } catch (_) {
      ctx.reasoning = "";
    }
    renderReasoningOptions();
    ctx.chat = null;
    ctx.chatStart = null;
    ctx.chatItems = [];
    ctx.chats = [];
    ctx.chatId = null;
    clearAttachments();
    setBusy(false);

    $("workspace").hidden = false;
    $("apps-panel").hidden = true;
    const actions = $("ws-actions");
    if (actions) actions.hidden = false;
    const back = $("ws-back");
    if (back) back.hidden = false;
    activateTab("apps");
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

    showWorkspaceTab("chat");
    $("chat-log").innerHTML = "";
    renderChatList();
    renderChat([]);
    renderChatMeta();
    ensureChat().catch((e) => {
      // A start for an app the user already left must not post an error into
      // the app that replaced it.
      if (ctx.appId === appId) chatError(String(e));
    });

    $("ws-title").textContent = ctx.appMeta.name || appId;
    $("ws-title").hidden = false;
    setViewTitle(ctx.appMeta.name || appId);
    const wsFolder = $("ws-folder");
    if (wsFolder) wsFolder.textContent = info.workspace || "—";

    if (hasMcp) renderToolSelectors();
    await refreshAppSettings();
  });
}

async function closeApp() {
  await guard(async () => {
    if (ctx.appId) {
      await invoke("chat_stop", { app_id: ctx.appId });
      await invoke("close_app", { app_id: ctx.appId });
    }
    await invoke("close_all_apps");
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (item) => item.app_id !== ctx.appId,
    );
    renderPermissionPrompt();
    ctx.appId = null;
    ctx.appMeta = null;
    ctx.chat = null;
    ctx.chatStart = null;
    // A closed session must not leave clickable history behind: the list is
    // bound to the app, and `chat_select` cannot run without one.
    ctx.chats = [];
    ctx.chatId = null;
    ctx.chatItems = [];
    ctx.chatQuery = "";
    const search = $("chat-search");
    if (search) search.value = "";
    renderChatList();
    $("chat-log").innerHTML = "";
    ctx.rendered = [];
    $("chat-list").scrollTop = 0;
    $("apps-panel").hidden = false;
    $("app-switch").hidden = true;
    $("workspace").hidden = true;
    const actions = $("ws-actions");
    if (actions) actions.hidden = true;
    const back = $("ws-back");
    if (back) back.hidden = true;
    const icon = $("ws-app-icon");
    if (icon) icon.textContent = "AI";
    $("ws-title").textContent = "Агент не выбран";
    $("ws-publisher").textContent = "выберите приложение в каталоге";
    const folder = $("ws-folder");
    if (folder) folder.textContent = "—";
    renderPermissionPrompt();
    setViewTitle("Каталог агентов");
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

async function openWorkspaceFile(name, action) {
  if (!name || !ctx.appId) return;
  try {
    if (action === "reveal") {
      await invoke("workspace_file_reveal", { app_id: ctx.appId, path: name });
      toast(`Показано в папке: ${name}`);
    } else if (action === "save") {
      const dest = await invoke("workspace_file_save_copy", { app_id: ctx.appId, path: name });
      toast(dest ? `Копия сохранена: ${dest}` : "Сохранение отменено");
    } else {
      await invoke("workspace_file_open", { app_id: ctx.appId, path: name });
      toast(`Открываю ${name}`);
    }
  } catch (e) {
    toast(String(e), true);
  }
}

async function openExternal(url) {
  if (!url) return;
  try {
    await invoke("open_external", { url });
    toast("Ссылка открыта в браузере");
  } catch (e) {
    toast(String(e), true);
  }
}

async function openWorkspaceFolder() {
  if (!ctx.appId) return;
  try {
    await invoke("workspace_open_folder", { app_id: ctx.appId });
    const folder = $("ws-folder");
    toast(`Открываю рабочую папку: ${folder ? folder.textContent : ""}`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function removeApp(appId) {
  const app = (ctx.apps || []).find((item) => item.app_id === appId);
  const ok = await askConfirm({
    title: "Удалить приложение?",
    message: `${(app && app.name) || appId} будет удалён из каталога вместе с пакетом. Рабочая папка останется на диске.`,
    confirmLabel: "Удалить",
    danger: true,
  });
  if (!ok) return;
  await guard(async () => {
    if (ctx.appId === appId) await closeApp();
    await invoke("remove_app", { app_id: appId });
    toast("Удалено");
    await refreshApps();
  });
}

async function rollbackApp(appId, version) {
  const ok = await askConfirm({
    title: "Откатить версию?",
    message: `${appId} вернётся к версии ${version}. Текущая версия будет заменена, разрешения пакета нужно будет выдать заново.`,
    confirmLabel: "Откатить",
    danger: true,
  });
  if (!ok) return;
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

function unescapeHtml(value) {
  return String(value).replace(/&(amp|lt|gt|quot|#39);/g, (_, key) => ({
    amp: "&",
    lt: "<",
    gt: ">",
    quot: '"',
    "#39": "'",
  }[key]));
}

// Workspace artifacts the agent mentions become actionable chips. Only
// document, image and text extensions are offered; the backend re-checks the
// path and refuses to shell-open scripts and executables.
const WS_FILE_EXT =
  /\.(md|markdown|txt|log|csv|tsv|json|jsonl|yaml|yml|xml|css|pdf|docx|xlsx|pptx|odt|ods|png|jpe?g|gif|webp|bmp|drawio)$/i;

// Input is already HTML-escaped by renderMarkdown, so the value is reused as is.
function workspaceFileRef(value) {
  const raw = String(value).trim();
  if (!raw || raw.length > 200 || /[\s<>]/.test(raw)) return "";
  if (/^[a-z][a-z0-9+.-]*:/i.test(raw) || raw.includes("..")) return "";
  // A bare extension in backticks (`.docx`, `.md`) is prose about a format, not
  // a file name: it must stay inline code instead of becoming a broken chip.
  if (/^\.[A-Za-z0-9]+$/.test(raw)) return "";
  const base = raw.slice(0, raw.lastIndexOf("."));
  if (!base || !/[A-Za-z0-9_\u0400-\u04FF]/.test(base.replace(/\/+$/, ""))) return "";
  if (!WS_FILE_EXT.test(raw)) return "";
  return raw.replace(/^\.\//, "");
}

const WS_ICON_REVEAL =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>';
const WS_ICON_SAVE =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v10" /><path d="M8 11l4 4 4-4" /><path d="M4 20h16" /></svg>';

function fileChipHtml(name) {
  return `<span class="ws-file"><button type="button" class="ws-file-name" data-ws-file="${name}" title="Открыть файл">${name}</button><button type="button" class="ws-file-act" data-ws-file="${name}" data-ws-action="reveal" title="Показать в папке" aria-label="Показать ${name} в папке">${WS_ICON_REVEAL}</button><button type="button" class="ws-file-act" data-ws-file="${name}" data-ws-action="save" title="Сохранить копию…" aria-label="Сохранить копию ${name}">${WS_ICON_SAVE}</button></span>`;
}

function linkifyBareFiles(text) {
  return text.replace(
    /(?<![\w.\-/])((?:[\w.-]+\/)*[\w.-]+\.[A-Za-z0-9]{1,6})(?![\w-])/g,
    (match, token) => {
      const name = workspaceFileRef(token);
      return name ? fileChipHtml(name) : match;
    },
  );
}

/* ---------- external links ---------- */

// Only http(s) and mailto are rendered as links; the backend re-validates the
// scheme and hands the URL to the system handler, never to the webview.
const EXT_URL_RE = /\b(?:https?:\/\/|mailto:)[^\s<>"'`]+/gi;

function trimUrlEnd(value) {
  let url = value;
  while (/[.,;:!?»"”']$/.test(url)) url = url.slice(0, -1);
  if (url.endsWith(")") && !url.includes("(")) url = url.slice(0, -1);
  return url;
}

function linkifyUrls(text) {
  return text.replace(EXT_URL_RE, (match) => {
    const url = trimUrlEnd(match);
    if (!url) return match;
    const tail = match.slice(url.length);
    return `<a class="ext-link" href="#" data-ext-link="${url}" rel="noreferrer noopener">${url}</a>${tail}`;
  });
}

function linkifyBareFilesOutsideLinks(html) {
  return html
    .split(/(<a\b[^>]*>[\s\S]*?<\/a>)/)
    .map((part, index) => (index % 2 ? part : linkifyBareFiles(part)))
    .join("");
}

function inlineMd(text) {
  // Code spans are handled first and stay literal: image syntax inside
  // backticks is a code sample, not a picture.
  return String(text)
    .split(/(`[^`]+`)/)
    .map((part) => {
      if (part.length > 2 && part.startsWith("`") && part.endsWith("`")) {
        const inner = part.slice(1, -1);
        const name = workspaceFileRef(inner);
        return name ? fileChipHtml(name) : `<code>${inner}</code>`;
      }
      return inlineNonCodeMd(part);
    })
    .join("");
}

function inlineNonCodeMd(text) {
  // Pull images out before linkification: a data:/blob: URI must not be
  // rewritten as a URL or a file chip. Placeholders are restored after the
  // emphasis pass so `*` inside an attribute is never treated as markup.
  const images = [];
  const withImages = String(text).replace(
    /!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g,
    (_match, alt, src) => {
      const safe = safeImageSrc(src);
      if (safe) {
        images.push(
          `<img class="md-image" src="${safe}" alt="${esc(alt)}" loading="lazy">`
        );
        return `\u0000img${images.length - 1}\u0000`;
      }
      const name = workspaceFileRef(src);
      if (name) {
        images.push(fileChipHtml(name));
        return `\u0000img${images.length - 1}\u0000`;
      }
      images.push(esc(`![${alt}](${src})`));
      return `\u0000img${images.length - 1}\u0000`;
    }
  );
  return linkifyBareFilesOutsideLinks(linkifyUrls(withImages))
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\u0000img(\d+)\u0000/g, (_match, index) => images[Number(index)] || "");
}

/// The closed contour does not fetch remote images, and the CSP only allows
/// `self`, `data:` and `blob:`. Anything else is rendered as text or a file chip.
function safeImageSrc(raw) {
  const src = String(raw || "").trim();
  if (/^data:image\/[a-z0-9.+-]+;base64,[a-z0-9+/=\s]+$/i.test(src)) return src;
  if (/^blob:/i.test(src)) return src;
  return "";
}

const DIAGRAM_KINDS = new Set(["mermaid", "chart"]);

/* ---------- syntax highlighting (offline, vendored highlight.js) ---------- */

const HLJS_MAX_CHARS = 60000;
const HLJS_AUTO_LANGS = [
  "javascript",
  "typescript",
  "python",
  "rust",
  "bash",
  "powershell",
  "json",
  "yaml",
  "sql",
  "xml",
  "css",
  "ini",
];
const HLJS_LANG_ALIAS = {
  "c++": "cpp",
  cplusplus: "cpp",
  "c#": "csharp",
  cs: "csharp",
  "objective-c": "objectivec",
  golang: "go",
  rs: "rust",
  kt: "kotlin",
  py: "python",
  md: "markdown",
  yml: "yaml",
  jsonc: "json",
  json5: "json",
  toml: "ini",
  env: "ini",
  dotenv: "ini",
  conf: "ini",
  console: "shell",
  terminal: "shell",
  "shell-session": "shell",
  cmd: "powershell",
  bat: "powershell",
  batch: "powershell",
};

// Fence text is HTML-escaped by renderMarkdown, while highlight.js expects raw
// source and escapes its own output.
function highlightCode(raw, lang) {
  const hljs = window.hljs;
  if (!hljs || raw.length > HLJS_MAX_CHARS) return "";
  const normalized = lang ? HLJS_LANG_ALIAS[lang] || lang : "";
  try {
    if (normalized && hljs.getLanguage(normalized)) {
      return hljs.highlight(raw, { language: normalized, ignoreIllegals: true }).value;
    }
    // Guessing on a two-word snippet paints noise, so only auto-detect
    // unlabelled blocks that are long enough to have a shape — and short enough
    // to stay cheap while a message is still streaming.
    if (normalized || raw.trim().length < 40 || raw.length > 4000) return "";
    return hljs.highlightAuto(raw, HLJS_AUTO_LANGS).value;
  } catch (_) {
    return "";
  }
}

function diagramBlockHtml(kind, code) {
  const label = kind === "mermaid" ? "Mermaid" : "Chart.js";
  return `<div class="diagram-block" data-diagram="${kind}" data-pending="1">
    <div class="diagram-toolbar">
      <span class="diagram-label">${label}</span>
      <span class="diagram-spacer"></span>
      <button class="ghost small" type="button" data-diagram-action="source">Код</button>
      ${kind === "mermaid" ? '<button class="ghost small" type="button" data-diagram-action="svg">SVG</button>' : ""}
      <button class="ghost small" type="button" data-diagram-action="png">PNG</button>
    </div>
    <div class="diagram-canvas"><div class="diagram-loading">Рендер схемы…</div></div>
    <pre class="diagram-source" hidden>${code}</pre>
  </div>`;
}

function splitTableRow(line) {
  const raw = line.trim();
  const cells = [];
  let cell = "";
  let inCode = false;
  for (const ch of raw) {
    if (ch === "`") inCode = !inCode;
    if (ch === "|" && !inCode) {
      cells.push(cell);
      cell = "";
    } else {
      cell += ch;
    }
  }
  cells.push(cell);
  if (raw.startsWith("|")) cells.shift();
  if (raw.endsWith("|")) cells.pop();
  return cells.map((value) => value.trim());
}

function tableAlignment(cell) {
  const value = cell.trim();
  if (!/^:?-{3,}:?$/.test(value)) return null;
  const left = value.startsWith(":");
  const right = value.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  if (left) return "left";
  return "";
}

function tableHtml(headers, aligns, rows) {
  const cell = (tag, value, align) =>
    `<${tag}${align ? ` style="text-align:${align}"` : ""}>${inlineMd(value || "")}</${tag}>`;
  const head = headers.map((value, index) => cell("th", value, aligns[index])).join("");
  const body = rows
    .map(
      (row) =>
        `<tr>${headers
          .map((_, index) => cell("td", row[index] || "", aligns[index]))
          .join("")}</tr>`,
    )
    .join("");
  return `<div class="md-table-wrap"><table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table></div>`;
}

function renderMarkdown(source, options) {
  const opts = options || {};
  const highlight = opts.highlight !== false;
  const lines = escapeHtml(source).split("\n");
  let html = "";
  let inCode = false;
  let codeLang = "";
  let code = [];
  let list = null;
  const closeList = () => {
    if (list) {
      html += `</${list}>`;
      list = null;
    }
  };
  const flushCode = () => {
    const body = code.join("\n");
    if (DIAGRAM_KINDS.has(codeLang) && body.trim()) {
      html += diagramBlockHtml(codeLang, body);
    } else if (body) {
      const label = codeLang || "code";
      const highlighted = highlight ? highlightCode(unescapeHtml(body), codeLang) : "";
      const classes = [codeLang ? `language-${codeLang}` : "", highlighted ? "hljs" : ""]
        .filter(Boolean)
        .join(" ");
      const attr = classes ? ` class="${escapeHtml(classes)}"` : "";
      html += `<div class="md-code">
        <div class="md-code-head">
          <span>${escapeHtml(label)}</span>
          <button class="ghost small" type="button" data-copy-code>Копировать</button>
        </div>
        <pre><code${attr}>${highlighted || body}</code></pre>
      </div>`;
    }
    code = [];
    codeLang = "";
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].replace(/\s+$/, "");
    if (line.trimStart().startsWith("```")) {
      if (inCode) {
        flushCode();
        inCode = false;
      } else {
        closeList();
        inCode = true;
        codeLang = line.trim().slice(3).trim().split(/\s+/)[0].toLowerCase();
      }
      continue;
    }
    if (inCode) {
      code.push(line);
      continue;
    }
    if (!line.trim()) {
      closeList();
      continue;
    }

    if (line.includes("|") && i + 1 < lines.length) {
      const delimiter = lines[i + 1].replace(/\s+$/, "");
      if (delimiter.includes("|")) {
        const headers = splitTableRow(line);
        const aligns = splitTableRow(delimiter).map(tableAlignment);
        if (
          headers.length &&
          aligns.length === headers.length &&
          aligns.every((align) => align !== null)
        ) {
          closeList();
          const rows = [];
          let next = i + 2;
          while (next < lines.length) {
            const row = lines[next].replace(/\s+$/, "");
            if (!row.trim() || !row.includes("|")) break;
            rows.push(splitTableRow(row));
            next += 1;
          }
          html += tableHtml(headers, aligns, rows);
          i = next - 1;
          continue;
        }
      }
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
  // An unfinished fence is still streaming: show it as code, not a half diagram.
  flushCode();
  return html;
}

/* ---------- diagrams (offline) ---------- */

const mermaidCache = new Map();
const activeCharts = new Set();
let mermaidTheme = null;
let mermaidSeq = 0;

function hashText(text) {
  let hash = 2166136261;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}

function currentTheme() {
  return document.documentElement.dataset.theme === "light" ? "default" : "dark";
}

function cleanupMermaidErrors() {
  for (const text of document.querySelectorAll(".error-text")) {
    const svg = text.ownerSVGElement;
    if (svg) svg.remove();
  }
}

function ensureMermaid() {
  if (!window.mermaid) return null;
  const theme = currentTheme();
  if (mermaidTheme !== theme) {
    window.mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme,
      fontFamily: "Golos Text",
      htmlLabels: false,
      suppressErrorRendering: true,
      flowchart: { htmlLabels: false, useMaxWidth: true },
      sequence: { useMaxWidth: true },
      maxTextSize: 100000,
      maxEdges: 1000,
    });
    mermaidTheme = theme;
    mermaidCache.clear();
    cleanupMermaidErrors();
  }
  return window.mermaid;
}

// Mermaid's xychart grammar requires quoted categorical labels. Models often
// emit `x-axis [янв, фев]`, which fails on Cyrillic bare words, so quote only
// that one axis form and leave every other diagram untouched.
function normalizeMermaid(source) {
  if (!/^\s*xychart-beta\b/i.test(source)) return source;
  return source
    .split("\n")
    .map((line) => {
      const match = line.match(/^(\s*x-axis\s+)\[(.*)\]\s*$/);
      if (!match) return line;
      const labels = match[2]
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean)
        .map((item) => {
          if (/^"(?:[^"\\]|\\.)*"$/.test(item) || /^'(?:[^'\\]|\\.)*'$/.test(item)) return item;
          if (/^-?\d+(?:\.\d+)?$/.test(item)) return item;
          return `"${item.replace(/"/g, '\\"')}"`;
        });
      return `${match[1]}[${labels.join(", ")}]`;
    })
    .join("\n");
}

function diagramError(block, message) {
  const canvas = block.querySelector(".diagram-canvas");
  if (!canvas) return;
  canvas.innerHTML = "";
  const err = document.createElement("div");
  err.className = "diagram-error";
  err.textContent = message;
  canvas.appendChild(err);
}

function setDiagramSvg(block, svg) {
  const canvas = block.querySelector(".diagram-canvas");
  if (!canvas) return;
  canvas.innerHTML = svg;
  block.removeAttribute("data-pending");
}

function renderMermaidBlock(block, source) {
  const mermaid = ensureMermaid();
  if (!mermaid) {
    diagramError(block, "Mermaid не загружен: проверьте vendor/mermaid.min.js");
    return;
  }
  if (source.length > 100000) {
    diagramError(block, "Схема слишком большая для безопасного рендера (лимит 100 КБ)");
    block.removeAttribute("data-pending");
    return;
  }
  const normalized = normalizeMermaid(source);
  const key = `${currentTheme()}\u0001${normalized}`;
  if (mermaidCache.has(key)) {
    setDiagramSvg(block, mermaidCache.get(key));
    return;
  }
  const id = `diagram-${hashText(normalized)}-${++mermaidSeq}`;
  Promise.resolve()
    .then(() => mermaid.render(id, normalized))
    .then((result) => {
      const svg = result && result.svg ? result.svg : "";
      if (!svg) throw new Error("пустой SVG");
      mermaidCache.set(key, svg);
      if (mermaidCache.size > 80) mermaidCache.delete(mermaidCache.keys().next().value);
      if (block.isConnected) setDiagramSvg(block, svg);
    })
    .catch((error) => {
      cleanupMermaidErrors();
      if (block.isConnected) diagramError(block, `Не удалось построить схему: ${error.message || error}`);
      block.removeAttribute("data-pending");
    });
}

function chartThemeColor(name, fallback) {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

function renderChartBlock(block, source) {
  if (source.length > 200000) {
    diagramError(block, "Описание графика слишком большое (лимит 200 КБ)");
    block.removeAttribute("data-pending");
    return;
  }
  let spec;
  try {
    spec = JSON.parse(source);
  } catch (error) {
    diagramError(block, `Chart JSON: ${error.message}`);
    block.removeAttribute("data-pending");
    return;
  }
  if (!window.Chart) {
    diagramError(block, "Chart.js не загружен: проверьте vendor/chart.umd.min.js");
    return;
  }
  const canvas = block.querySelector(".diagram-canvas");
  if (!canvas) return;
  canvas.innerHTML = '<canvas aria-label="График"></canvas>';
  const element = canvas.querySelector("canvas");
  const config = spec.data
    ? { ...spec }
    : {
        type: spec.type || "bar",
        data: { labels: spec.labels || [], datasets: spec.datasets || [] },
        options: spec.options || {},
      };
  config.options = {
    responsive: true,
    maintainAspectRatio: false,
    ...(config.options || {}),
  };
  config.options.plugins = {
    ...(config.options.plugins || {}),
    legend: {
      ...((config.options.plugins || {}).legend || {}),
      labels: {
        color: chartThemeColor("--text-secondary", "#cbd5e1"),
        ...(((config.options.plugins || {}).legend || {}).labels || {}),
      },
    },
  };
  try {
    const chart = new window.Chart(element, config);
    activeCharts.add(chart);
    element.__chart = chart;
    block.removeAttribute("data-pending");
  } catch (error) {
    diagramError(block, `Chart.js: ${error.message}`);
    block.removeAttribute("data-pending");
  }
}

function destroyCharts(root) {
  if (!root || !root.querySelectorAll) return;
  for (const canvas of root.querySelectorAll("canvas")) {
    const chart = canvas.__chart;
    if (chart) {
      activeCharts.delete(chart);
      try {
        chart.destroy();
      } catch (_) {}
      canvas.__chart = null;
    }
  }
}

function enhanceDiagrams(root) {
  if (!root || !root.querySelectorAll) return;
  for (const block of root.querySelectorAll(".diagram-block[data-pending]")) {
    const source = (block.querySelector(".diagram-source") || {}).textContent || "";
    if (!source.trim()) {
      block.removeAttribute("data-pending");
      continue;
    }
    if (block.dataset.diagram === "mermaid") renderMermaidBlock(block, source);
    else if (block.dataset.diagram === "chart") renderChartBlock(block, source);
  }
}

function diagramFileName(block, ext) {
  const source = (block.querySelector(".diagram-source") || {}).textContent || "";
  return `${block.dataset.diagram || "diagram"}-${hashText(source)}.${ext}`;
}

async function saveDiagramFile(block, format, content) {
  if (!ctx.appId) return toast("Сначала откройте приложение", true);
  const path = await invoke("save_diagram", {
    app_id: ctx.appId,
    filename: diagramFileName(block, format),
    format,
    content,
  });
  if (path) toast(`Сохранено: ${path}`);
}

async function exportDiagram(block, kind) {
  const canvas = block.querySelector(".diagram-canvas");
  if (!canvas) return;
  if (kind === "svg") {
    const svg = canvas.querySelector("svg");
    if (!svg) return toast("SVG ещё не готов", true);
    const text = new XMLSerializer().serializeToString(svg);
    await saveDiagramFile(block, "svg", text);
    return;
  }
  if (kind === "png") {
    const svg = canvas.querySelector("svg");
    const sourceCanvas = canvas.querySelector("canvas");
    if (sourceCanvas) {
      await saveDiagramFile(block, "png", sourceCanvas.toDataURL("image/png").split(",")[1]);
      return;
    }
    if (!svg) return toast("Схема ещё не готова", true);
    const text = new XMLSerializer().serializeToString(svg);
    const image = new Image();
    const url = URL.createObjectURL(new Blob([text], { type: "image/svg+xml" }));
    try {
      await new Promise((resolve, reject) => {
        image.onload = resolve;
        image.onerror = () => reject(new Error("не удалось отрисовать SVG"));
        image.src = url;
      });
      const scale = 2;
      const width = Math.max(1, image.naturalWidth || svg.clientWidth || 800);
      const height = Math.max(1, image.naturalHeight || svg.clientHeight || 600);
      const out = document.createElement("canvas");
      out.width = width * scale;
      out.height = height * scale;
      const context = out.getContext("2d");
      context.scale(scale, scale);
      context.drawImage(image, 0, 0, width, height);
      await saveDiagramFile(block, "png", out.toDataURL("image/png").split(",")[1]);
    } catch (error) {
      toast(error.message, true);
    } finally {
      URL.revokeObjectURL(url);
    }
  }
}

function handleDiagramAction(event) {
  const button = event.target.closest("[data-diagram-action]");
  if (!button) return;
  const block = button.closest(".diagram-block");
  if (!block) return;
  const action = button.dataset.diagramAction;
  if (action === "source") {
    const source = block.querySelector(".diagram-source");
    if (source) source.hidden = !source.hidden;
    return;
  }
  guard(() => exportDiagram(block, action));
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
    (item.text || "").length,
    (item.text || "").slice(0, 32),
    (item.text || "").slice(-32),
    item.tokens_in ?? "",
    item.tokens_out ?? "",
    item.reasoning_tokens ?? "",
    item.ms ?? "",
    item.tok_s ?? "",
    item.model || "",
    item.provider || "",
    item.message_id || "",
  ].join("\u0001");
}

const ICON_EDIT =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>';
const ICON_COPY =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg>';
const ICON_REGENERATE =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/></svg>';
const ICON_REASONING =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M12 2a7 7 0 0 0-4 12.7V18h8v-3.3A7 7 0 0 0 12 2z"/></svg>';

function iconButton(html, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "ghost small icon-btn";
  button.innerHTML = html;
  button.title = label;
  button.setAttribute("aria-label", label);
  return button;
}

function previousUserMessage(items, index) {
  for (let i = index - 1; i >= 0; i--) {
    const item = items[i];
    if (item.role === "user" && item.kind === "text" && item.message_id) return item;
  }
  return null;
}

function buildChatNode(item, streaming, items, index) {
  if (item.role === "tool") {
    const div = document.createElement("div");
    div.className = "msg tool";
    div.textContent = `⚙ tool: ${item.text}`;
    return div;
  }

  if (item.kind === "file") {
    const div = document.createElement("div");
    div.className = "msg " + (item.role === "user" ? "user" : "assistant");
    const isImage = item.mime && item.mime.startsWith("image/");
    const chip = () => {
      const node = document.createElement("div");
      node.className = "chat-attachment-chip";
      const kind = isImage ? "изображение" : "файл";
      node.textContent = `${kind}: ${item.text}`;
      return node;
    };
    if (isImage && (item.data || item.image_ref)) {
      const figure = document.createElement("figure");
      figure.className = "chat-figure";
      const img = document.createElement("img");
      img.className = "chat-image";
      img.loading = "lazy";
      img.alt = item.text || "изображение";
      const caption = document.createElement("figcaption");
      caption.textContent = item.text || "";
      figure.appendChild(img);
      figure.appendChild(caption);
      if (item.data) {
        img.src = `data:${item.mime};base64,${item.data}`;
      } else {
        // The history stores only a content hash; load the picture on demand.
        invoke("chat_image", { app_id: ctx.appId, image_ref: item.image_ref })
          .then((image) => {
            if (image && image.data) {
              img.src = `data:${image.mime || item.mime};base64,${image.data}`;
            } else {
              figure.replaceWith(chip());
            }
          })
          .catch(() => figure.replaceWith(chip()));
      }
      div.appendChild(figure);
      return div;
    }
    div.appendChild(chip());
    return div;
  }

  if (item.kind === "reasoning") {
    const details = document.createElement("details");
    details.className = "msg reasoning";
    const summary = document.createElement("summary");
    summary.textContent = "Ход размышлений модели";
    const body = document.createElement("div");
    body.className = "msg-body";
    body.innerHTML = renderMarkdown(item.text, { highlight: !streaming });
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
    body.innerHTML = renderMarkdown(item.text, { highlight: !streaming });
  }
  div.appendChild(body);

  if (item.role === "user" && item.kind === "text") {
    const footer = document.createElement("div");
    footer.className = "msg-foot user-actions";
    if (item.message_id) {
      const edit = iconButton(ICON_EDIT, "Изменить сообщение");
      edit.dataset.editMessage = String(index);
      footer.appendChild(edit);
    }
    const copy = iconButton(ICON_COPY, "Копировать сообщение");
    copy.dataset.copy = "1";
    copy.dataset.text = item.text;
    footer.appendChild(copy);
    div.appendChild(footer);
  }

  if (item.role === "assistant") {
    const footer = document.createElement("div");
    footer.className = "msg-foot";
    const meta = document.createElement("span");
    meta.className = "muted small mono";
    const add = (text) => {
      if (meta.childNodes.length) meta.append(" · ");
      meta.append(document.createTextNode(text));
    };
    const addReasoning = (value) => {
      if (meta.childNodes.length) meta.append(" · ");
      const wrap = document.createElement("span");
      wrap.className = "stat-with-icon";
      wrap.title = "Токены рассуждений";
      wrap.setAttribute("aria-label", `Токены рассуждений: ${fmtNum(value)}`);
      wrap.innerHTML = `${ICON_REASONING}<span>${fmtNum(value)}</span>`;
      meta.appendChild(wrap);
    };
    if (item.tokens_in != null) add(`↑${fmtNum(item.tokens_in)}`);
    const outputTokens =
      (Number(item.tokens_out) || 0) + (Number(item.reasoning_tokens) || 0);
    if (item.tokens_out != null || item.reasoning_tokens) {
      add(`↓${fmtNum(outputTokens)}`);
    }
    if (item.reasoning_tokens) addReasoning(item.reasoning_tokens);
    if (item.tok_s) add(`${Math.round(item.tok_s)} tok/s`);
    if (item.ms) add(`${(item.ms / 1000).toFixed(1)} с`);
    if (item.model) add(displayModelName(item));
    const cost = itemCost(item);
    if (cost) add(`≈${fmtCost(cost.amount, cost.currency)}`);
    const copy = iconButton(ICON_COPY, "Копировать ответ");
    copy.dataset.copy = "1";
    copy.dataset.text = item.text;
    footer.appendChild(meta);
    footer.appendChild(copy);
    if (previousUserMessage(items, index)) {
      const regen = iconButton(ICON_REGENERATE, "Перегенерировать ответ");
      regen.dataset.regenerate = String(index);
      footer.appendChild(regen);
    }
    div.appendChild(footer);
  }

  return div;
}

// Incremental rendering: polling fires every 700 ms and most items do not
// change. Rebuilding the whole log made messages blink and reset the scroll.
function nearLogBottom(log) {
  return log.scrollHeight - log.scrollTop - log.clientHeight < 48;
}

function renderChat(items) {
  const log = $("chat-log");
  const rendered = ctx.rendered || (ctx.rendered = []);
  const atBottom = nearLogBottom(log);

  if (!items || !items.length) {
    if (log.querySelector(".chat-empty")) return;
    destroyCharts(log);
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
    updateContextMeter([]);
    return;
  }

  if (log.querySelector(".chat-empty")) {
    destroyCharts(log);
    log.innerHTML = "";
    rendered.length = 0;
  }

  for (let i = 0; i < items.length; i++) {
    const fingerprint = chatItemKey(items[i]);
    const entry = rendered[i];
    const current = log.children[i];
    // The last item may still be streaming; always rebuild it so equal-length
    // deltas are not missed. Older items are stable and keyed cheaply.
    if (i < items.length - 1 && entry && entry.fingerprint === fingerprint && entry.node === current) continue;

    // Live snapshots arrive every 80 ms: skip highlighting the growing message
    // and let the final `chat://update` render it once.
    const streaming = !!ctx.busy && !!ctx.acceptStream && i === items.length - 1;
    const node = buildChatNode(items[i], streaming, items, i);
    if (entry && entry.node && entry.node.tagName === "DETAILS" && entry.node.open) {
      node.open = true;
    }
    if (current) {
      destroyCharts(current);
      log.replaceChild(node, current);
    } else {
      log.appendChild(node);
    }
    rendered[i] = { fingerprint, node };
  }

  while (rendered.length > items.length) {
    const entry = rendered.pop();
    if (entry.node.parentNode === log) {
      destroyCharts(entry.node);
      log.removeChild(entry.node);
    }
  }
  while (log.children.length > items.length) {
    const last = log.lastElementChild;
    if (last && last.classList.contains("chat-thinking")) break;
    destroyCharts(last);
    log.removeChild(last);
  }

  enhanceDiagrams(log);
  updateContextMeter(items);
  if (atBottom) log.scrollTop = log.scrollHeight;
  if (ctx.busy) renderThinking();
}

// Shows how much of the model context window the last turn occupied, how many
// of those tokens came from the provider cache, and (in the tooltip) the cache
// hit rate. The prompt usage comes from the assistant message; the window size
// comes from the model capabilities configured for the gateway.
function modelForItem(item) {
  if (!item || !item.model) return null;
  const providers = effectiveProviders();
  let provider = item.provider
    ? providers.find((candidate) => candidate.id === item.provider)
    : null;
  if (!provider) {
    const owners = providers.filter((candidate) =>
      candidate.models.some((model) => model.id === item.model),
    );
    if (owners.length === 1) provider = owners[0];
  }
  return provider ? provider.models.find((model) => model.id === item.model) : null;
}

function displayModelName(item) {
  const model = modelForItem(item);
  return (model && model.name) || (item && item.model) || "";
}

function pricingForItem(item) {
  const model = modelForItem(item);
  return model ? model.pricing : null;
}

function itemCost(item) {
  const pricing = pricingForItem(item);
  if (!pricing) return null;
  // Both directions are needed for a meaningful estimate: a missing output
  // price would silently make every answer free. Cache prices may be omitted;
  // cached tokens are then charged at the input rate (conservative).
  if (pricing.input_per_mtok == null || pricing.output_per_mtok == null) {
    return null;
  }
  const input = Number(pricing.input_per_mtok) || 0;
  const output = Number(pricing.output_per_mtok) || 0;
  const cacheRead =
    pricing.cache_read_per_mtok == null
      ? input
      : Number(pricing.cache_read_per_mtok) || 0;
  const cacheWrite =
    pricing.cache_write_per_mtok == null
      ? input
      : Number(pricing.cache_write_per_mtok) || 0;
  // Providers bill reasoning as output; the engine reports it separately.
  const outputTokens =
    (Number(item.tokens_out) || 0) + (Number(item.reasoning_tokens) || 0);
  const amount =
    ((Number(item.tokens_in) || 0) * input +
      outputTokens * output +
      (Number(item.cache_read) || 0) * cacheRead +
      (Number(item.cache_write) || 0) * cacheWrite) /
    1_000_000;
  return { amount, currency: pricing.currency || "USD" };
}

function fmtCost(amount, currency) {
  if (amount == null) return "—";
  const abs = Math.abs(amount);
  const digits = abs >= 1 ? 2 : abs >= 0.01 ? 4 : 6;
  return `${amount.toFixed(digits)} ${currency || "USD"}`;
}

function sessionUsage(items) {
  const totals = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
  let cost = 0;
  let currency = "";
  let priced = false;
  for (const item of items || []) {
    if (item.role !== "assistant") continue;
    totals.input += Number(item.tokens_in) || 0;
    totals.output +=
      (Number(item.tokens_out) || 0) + (Number(item.reasoning_tokens) || 0);
    totals.cacheRead += Number(item.cache_read) || 0;
    totals.cacheWrite += Number(item.cache_write) || 0;
    const itemPrice = itemCost(item);
    if (itemPrice) {
      cost += itemPrice.amount;
      currency = itemPrice.currency || currency;
      priced = true;
    }
  }
  return { totals, cost: priced ? cost : null, currency: currency || "USD" };
}

function updateContextMeter(items) {
  const chip = $("chat-context");
  const text = $("chat-context-text");
  if (!chip || !text) return;
  let usage = null;
  for (let i = (items || []).length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.role === "user") continue;
    if (item.tokens_in == null && item.cache_read == null && item.cache_write == null) continue;
    usage = contextUsage(item);
    break;
  }
  if (!usage) {
    chip.hidden = true;
    closeContextPopover();
    return;
  }
  ctx.meterItems = items || [];
  const info = selectedModelInfo();
  const cap = Number((info && info.model.capabilities.context_tokens) || 0);
  ctx.contextUsage = usage;
  ctx.contextLimit = cap;
  chip.hidden = false;
  chip.classList.remove("warn", "bad");
  chip.title = "Заполнение контекстного окна — нажмите для подробностей";
  if (!cap) {
    text.textContent = `контекст ${fmtNum(usage.used)}`;
  } else {
    const percent = Math.round((usage.used / cap) * 1000) / 10;
    chip.classList.toggle("warn", percent >= 75 && percent < 90);
    chip.classList.toggle("bad", percent >= 90);
    text.textContent = `контекст ${percent}%`;
  }
  renderContextPopover();
  const compact = $("chat-compact");
  if (compact) compact.disabled = !ctx.chatId || ctx.busy || ctx.compacting;
}

function compactionPercent() {
  const limit = Number(ctx.contextLimit) || 0;
  const used = Number(ctx.contextUsage && ctx.contextUsage.used) || 0;
  if (!limit || !used) return 0;
  return (used / limit) * 100;
}

async function compactContext() {
  if (!ctx.appId || !ctx.chatId || ctx.busy || ctx.compacting) return;
  ctx.compacting = true;
  setBusy(true);
  try {
    await invoke("chat_compact", { app_id: ctx.appId });
    await loadHistory();
    updateContextMeter(ctx.chatItems || []);
    toast("Контекст сжат");
  } catch (e) {
    chatError(String(e));
  } finally {
    ctx.compacting = false;
    setBusy(false);
  }
}

/// Auto-compaction is off by default and runs only after a finished turn.
/// The threshold is a percentage of the selected model's context window.
async function maybeAutoCompact() {
  if (!ctx.appId || !ctx.chatId || ctx.busy || ctx.compacting) return;
  if (!(ctx.config && ctx.config.compaction_auto)) return;
  const threshold = Number(ctx.config.compaction_threshold_percent) || 80;
  const untilKey = `eco.compactUntil.${ctx.chatId}`;
  let until = 0;
  try {
    until = Number(sessionStorage.getItem(untilKey) || 0);
  } catch (_) {}
  if (Date.now() < until) return;
  await loadHistory();
  if (compactionPercent() < threshold) return;
  try {
    sessionStorage.setItem(untilKey, String(Date.now() + 120000));
  } catch (_) {}
  await compactContext();
}

function contextRows(usage, cap) {
  const rows = [["Занято", fmtNum(usage.used)]];
  if (cap) {
    rows.push(["Окно", fmtNum(cap)]);
    rows.push(["Осталось", fmtNum(Math.max(0, cap - usage.used))]);
  } else {
    rows.push(["Окно", "лимит не задан"]);
  }
  if (usage.cached) {
    rows.push(["Закешировано", fmtNum(usage.cached)]);
    rows.push(["Попадание в кэш", `${usage.hitRate.toFixed(1)}%`]);
  }
  if (usage.cacheWrite) {
    rows.push(["Записано в кэш", fmtNum(usage.cacheWrite)]);
  }
  return rows;
}

function renderContextPopover() {
  const pop = $("chat-context-popover");
  const body = $("chat-context-body");
  if (!pop || !body || !ctx.contextUsage) return;
  body.replaceChildren();
  const rows = contextRows(ctx.contextUsage, ctx.contextLimit || 0);
  const session = sessionUsage(ctx.meterItems || ctx.chatItems || []);
  rows.push([
    "Токены сессии",
    `↑${fmtNum(session.totals.input)} ↓${fmtNum(session.totals.output)}`,
  ]);
  if (session.totals.cacheRead) {
    rows.push(["Кэш за сессию", fmtNum(session.totals.cacheRead)]);
  }
  if (session.cost != null) {
    rows.push(["Стоимость сессии (оценка)", fmtCost(session.cost, session.currency)]);
  }
  for (const [label, value] of rows) {
    const row = document.createElement("div");
    row.className = "context-row";
    const name = document.createElement("span");
    name.textContent = label;
    const val = document.createElement("strong");
    val.className = "mono";
    val.textContent = value;
    row.append(name, val);
    body.appendChild(row);
  }
}

function closeContextPopover() {
  const pop = $("chat-context-popover");
  const chip = $("chat-context");
  if (!pop || pop.hidden) return;
  pop.hidden = true;
  if (chip) chip.setAttribute("aria-expanded", "false");
}

function toggleContextPopover() {
  const pop = $("chat-context-popover");
  const chip = $("chat-context");
  if (!pop || !chip) return;
  if (!pop.hidden) {
    closeContextPopover();
    return;
  }
  if (!ctx.contextUsage) return;
  renderContextPopover();
  closeChatPops("chat-context-popover");
  placeChatPop(pop, chip, true);
  chip.setAttribute("aria-expanded", "true");
}

/// Providers disagree about cached prompt tokens: OpenAI-compatible ones count
/// them inside `input`, Anthropic-style ones report `input` without the cache.
/// A cache read larger than the fresh input can only be the latter, so the
/// cache is added; otherwise `input` already is the whole window. The cached
/// count and the hit rate stay separate from the occupied size.
function contextUsage(item) {
  const input = item.tokens_in == null ? 0 : item.tokens_in;
  const cacheRead = item.cache_read || 0;
  const cacheWrite = item.cache_write || 0;
  const used = cacheRead > input ? input + cacheRead + cacheWrite : input + cacheWrite;
  if (!used && !cacheRead) return null;
  return {
    used,
    cached: cacheRead,
    cacheWrite,
    hitRate: used > 0 ? (cacheRead / used) * 100 : 0,
  };
}

function fmtNum(value) {
  if (value >= 1000000) return (value / 1000000).toFixed(1) + "M";
  if (value >= 1000) return (value / 1000).toFixed(1) + "k";
  return String(value);
}

function chatInterrupted(message) {
  const log = $("chat-log");
  const stick = nearLogBottom(log);
  const div = document.createElement("div");
  div.className = "msg error interrupted";
  const text = document.createElement("span");
  text.textContent = `Ход прерван: ${message}`;
  const retry = document.createElement("button");
  retry.type = "button";
  retry.className = "ghost small";
  retry.dataset.retryTurn = "1";
  retry.textContent = "Повторить";
  div.append(text, retry);
  log.appendChild(div);
  if (stick) log.scrollTop = log.scrollHeight;
}

function chatError(message) {
  const log = $("chat-log");
  const stick = nearLogBottom(log);
  // Engine-start failures can be reported by more than one caller (openApp and
  // dispatchTurn). Do not stack identical banners back to back.
  const last = log.lastElementChild;
  if (
    last &&
    last.classList.contains("msg") &&
    last.classList.contains("error") &&
    last.textContent === message
  ) {
    if (stick) log.scrollTop = log.scrollHeight;
    return;
  }
  const div = document.createElement("div");
  div.className = "msg error";
  div.textContent = message;
  log.appendChild(div);
  if (stick) log.scrollTop = log.scrollHeight;
}

let busyTimer = null;
let busyStart = 0;

function setBusy(on) {
  ctx.busy = Boolean(on);
  const stop = $("chat-stop");
  const send = $("chat-send");
  if (stop) {
    stop.hidden = !on;
    stop.style.display = on ? "" : "none";
  }
  if (send) {
    send.hidden = false;
    send.style.display = on ? "none" : "";
  }
  if (on) {
    busyStart = Date.now();
    clearInterval(busyTimer);
    renderThinking();
    busyTimer = setInterval(renderThinking, 1000);
  } else {
    clearInterval(busyTimer);
    busyTimer = null;
    renderThinking();
  }
  const compact = $("chat-compact");
  if (compact) compact.disabled = on || !ctx.chatId || ctx.compacting;
}

function renderThinking() {
  const log = $("chat-log");
  if (!log) return;
  let node = log.querySelector(".chat-thinking");
  if (!ctx.busy) {
    if (node) node.remove();
    return;
  }
  // Follow the tail only while the user is already there: scrolling up during
  // a long turn must not be undone by the once-a-second refresh.
  const stick = nearLogBottom(log);
  if (!node) {
    node = document.createElement("div");
    node.className = "chat-thinking";
    node.innerHTML = '<span class="spinner" aria-hidden="true"></span><span class="chat-thinking-text"></span>';
  }
  const secs = Math.max(0, Math.round((Date.now() - busyStart) / 1000));
  node.querySelector(".chat-thinking-text").textContent = secs ? `думает… ${secs} с` : "думает…";
  if (log.lastElementChild !== node) log.appendChild(node);
  if (stick) log.scrollTop = log.scrollHeight;
}

function renderChatMeta(model) {
  const info = selectedModelInfo();
  const endpoint =
    (info && info.provider.endpoint) ||
    (ctx.config && ctx.config.llm_endpoint) ||
    "";
  renderModelChip(model);
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

/** Buckets a session timestamp into a history group label. */
function chatGroup(updated) {
  if (!updated) return "Ранее";
  const stamp = Number(updated) < 1e12 ? Number(updated) * 1000 : Number(updated);
  const date = new Date(stamp);
  if (Number.isNaN(date.getTime())) return "Ранее";
  const midnight = (value) =>
    new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  const diff = Math.round((midnight(new Date()) - midnight(date)) / 86400000);
  if (diff <= 0) return "Сегодня";
  if (diff === 1) return "Вчера";
  if (diff < 7) return "На этой неделе";
  return "Ранее";
}

function startChatRename(button, chat) {
  const input = document.createElement("input");
  input.className = "chat-rename";
  input.value = chat.title;
  input.setAttribute("aria-label", "Название чата");
  button.replaceWith(input);
  input.focus();
  input.select();
  let done = false;
  const finish = async (save) => {
    if (done) return;
    done = true;
    const value = input.value.trim();
    if (save && value && value !== chat.title) {
      try {
        await invoke("chat_rename", {
          app_id: ctx.appId,
          session_id: chat.id,
          title: value,
        });
        chat.title = value;
        toast("Чат переименован");
      } catch (e) {
        toast(String(e), true);
      }
    }
    renderChatList();
  };
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      finish(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      finish(false);
    }
  });
  input.addEventListener("blur", () => finish(true));
}

function pinnedChatsKey() {
  return `eco.pinned.${ctx.appId || ""}`;
}

function pinnedChatIds() {
  try {
    const list = JSON.parse(localStorage.getItem(pinnedChatsKey()) || "[]");
    return Array.isArray(list) ? list.filter((id) => typeof id === "string") : [];
  } catch (_) {
    return [];
  }
}

function togglePinnedChat(id) {
  const set = new Set(pinnedChatIds());
  if (set.has(id)) set.delete(id);
  else set.add(id);
  try {
    localStorage.setItem(pinnedChatsKey(), JSON.stringify([...set]));
  } catch (_) {}
  renderChatList();
}

function chatRow(chat, pinned) {
  const row = document.createElement("div");
  row.className = "chat-row";

  const btn = document.createElement("button");
  btn.className = "chat-item" + (chat.id === ctx.chatId ? " active" : "");
  btn.textContent = chat.title;
  btn.title = `${chat.title}\nДвойной щелчок — переименовать`;
  btn.dataset.chat = chat.id;
  btn.addEventListener("dblclick", (event) => {
    event.preventDefault();
    startChatRename(btn, chat);
  });

  const pin = document.createElement("button");
  pin.type = "button";
  pin.className = "chat-pin" + (pinned ? " active" : "");
  pin.title = pinned ? "Открепить чат" : "Закрепить чат";
  pin.setAttribute("aria-label", pin.title);
  pin.dataset.pin = chat.id;
  pin.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 17v5"/><path d="M9 10.8V6a3 3 0 0 1 6 0v4.8l2 2.2v1.5H7V13z"/></svg>';
  pin.addEventListener("click", (event) => {
    event.stopPropagation();
    togglePinnedChat(chat.id);
  });

  const del = document.createElement("button");
  del.className = "chat-del";
  del.textContent = "×";
  del.title = "Удалить чат";
  del.dataset.del = chat.id;

  row.appendChild(btn);
  row.appendChild(pin);
  row.appendChild(del);
  return row;
}

function renderChatList() {
  const box = $("chat-list");
  box.innerHTML = "";
  const query = (ctx.chatQuery || "").trim().toLowerCase();
  const chats = (ctx.chats || []).filter(
    (chat) => !query || String(chat.title || "").toLowerCase().includes(query),
  );
  const pinned = new Set(pinnedChatIds());
  const pinnedChats = chats.filter((chat) => pinned.has(chat.id));
  const otherChats = chats.filter((chat) => !pinned.has(chat.id));
  const addGroup = (label, list) => {
    if (!list.length) return;
    const head = document.createElement("div");
    head.className = "hist-group";
    head.textContent = label;
    box.appendChild(head);
    for (const chat of list) box.appendChild(chatRow(chat, pinned.has(chat.id)));
  };
  addGroup("Закреплённые", pinnedChats);
  let group = "";
  for (const chat of otherChats) {
    const label = chatGroup(chat.updated);
    if (label !== group) {
      group = label;
      const head = document.createElement("div");
      head.className = "hist-group";
      head.textContent = label;
      box.appendChild(head);
    }
    box.appendChild(chatRow(chat, false));
  }
  if (!chats.length && query) {
    const empty = document.createElement("p");
    empty.className = "muted small hist-empty";
    empty.textContent = "Ничего не найдено";
    box.appendChild(empty);
  }
}

function setViewTitle(text) {
  const title = document.getElementById("tb-title");
  if (title) title.textContent = text;
}

function setSheetOpen(open) {
  const scrim = document.getElementById("sheet-scrim");
  if (scrim) scrim.hidden = !open;
}

/// Settings opens as a sheet over whatever is visible. Closing it must return
/// to that view: a chat that was open stays open (with its history and engine),
/// instead of silently dropping to the catalog.
function closeSettings() {
  activateTab("apps");
  if (ctx.settingsReturn === "chat" && ctx.appId) {
    showWorkspace();
  } else {
    showCatalog();
  }
  ctx.settingsReturn = null;
}

function activateTab(name) {
  document
    .querySelectorAll(".tab")
    .forEach((tab) => tab.classList.toggle("active", tab.dataset.tab === name));
  document
    .querySelectorAll(".tab-body")
    .forEach((body) => body.classList.toggle("active", body.id === `tab-${name}`));
  setSheetOpen(name === "settings");
}

/** Shows the catalog without stopping the running app session. */
function showCatalog() {
  $("workspace").hidden = true;
  $("apps-panel").hidden = false;
  const actions = $("ws-actions");
  if (actions) actions.hidden = true;
  const back = $("ws-back");
  if (back) back.hidden = true;
  setViewTitle("Каталог агентов");
}

/** Switches between the chat, tools and settings tabs inside an open app. */
function showWorkspaceTab(name) {
  document
    .querySelectorAll(".ws-tab")
    .forEach((tab) => tab.classList.toggle("active", tab.dataset.ws === name));
  for (const key of ["chat", "tools", "settings"]) {
    const body = $(`ws-${key}`);
    if (body) body.hidden = key !== name;
  }
  if (name === "settings") guard(refreshAppSettings);
  if (name === "chat") {
    $("chat-text").focus();
  }
  const back = $("ws-back");
  if (back) {
    back.title = name === "chat" ? "Вернуться к каталогу" : "Вернуться к диалогу";
  }
}

/** Returns to the chat of the selected app (the engine keeps running). */
function showWorkspace() {
  if (!ctx.appId) return;
  activateTab("apps");
  $("workspace").hidden = false;
  $("apps-panel").hidden = true;
  const actions = $("ws-actions");
  if (actions) actions.hidden = false;
  const back = $("ws-back");
  if (back) back.hidden = false;
  setViewTitle((ctx.appMeta && ctx.appMeta.name) || ctx.appId);
}

/* ---------- sidebar: collapse, history rail, shortcuts ---------- */

const SIDEBAR_KEY = "sidebar-collapsed";

function sidebarCollapsed() {
  const app = document.querySelector(".app");
  return Boolean(app && app.classList.contains("collapsed"));
}

function setSidebarCollapsed(collapsed) {
  const app = document.querySelector(".app");
  if (!app) return;
  app.classList.toggle("collapsed", collapsed);
  if (!collapsed) app.classList.remove("peek");
  try {
    localStorage.setItem(SIDEBAR_KEY, collapsed ? "1" : "0");
  } catch (_) {}
  const btn = $("btn-collapse");
  if (btn) {
    const text = collapsed ? "Развернуть панель (Ctrl+B)" : "Свернуть панель (Ctrl+B)";
    btn.title = text;
    btn.setAttribute("aria-label", text);
  }
}

/** In the collapsed rail the history opens as an overlay over the chat. */
function openHistoryFromRail() {
  const app = document.querySelector(".app");
  const search = $("chat-search");
  if (!sidebarCollapsed()) {
    if (search) search.focus();
    return;
  }
  app.classList.add("peek");
  if (search) search.focus();
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
  // openApp and dispatchTurn can ask for the engine at the same time; share one
  // start attempt instead of racing two chat_open calls.
  if (ctx.chatStart && ctx.chatStart.app === ctx.appId) return ctx.chatStart.promise;
  const appId = ctx.appId;
  renderModelChip("запуск движка…");
  const promise = (async () => {
    const info = await invoke("chat_open", { app_id: appId });
    if (ctx.appId !== appId) return null;
    ctx.chat = { app: appId, model: info.model };
    ctx.chatId = ctx.chatId || info.session;
    renderChatMeta(info.model);
    await refreshChats();

    if (!ctx.chatId) {
      const created = await invoke("chat_new", { app_id: appId });
      ctx.chatId = created.session;
      await refreshChats();
    } else if (!(ctx.chats || []).some((c) => c.id === ctx.chatId) && (ctx.chats || []).length) {
      ctx.chatId = ctx.chats[0].id;
      await invoke("chat_select", { app_id: appId, session_id: ctx.chatId });
    }
    await loadHistory();
    renderPermissionPrompt();
    return ctx.chat;
  })();
  ctx.chatStart = { app: appId, promise };
  try {
    return await promise;
  } catch (e) {
    // Do not leave "запуск движка…" forever after a failed start.
    if (ctx.appId === appId) {
      renderChatMeta();
      const chip = $("chat-model-btn");
      if (chip) chip.title = String(e);
    }
    throw e;
  } finally {
    if (ctx.chatStart && ctx.chatStart.promise === promise) ctx.chatStart = null;
  }
}

async function newChat() {
  if (!ctx.appId) {
    toast("Сначала выберите приложение в каталоге", true);
    return;
  }
  await guard(async () => {
    const appId = ctx.appId;
    showWorkspace();
    const chat = await ensureChat().catch(() => null);
    if (!chat || ctx.appId !== appId) return;
    const info = await invoke("chat_new", { app_id: appId });
    if (ctx.appId !== appId) return;
    ctx.chatId = info.session;
    ctx.chatItems = [];
    ctx.acceptStream = false;
    ctx.streamingItems = [];
    setBusy(false);
    renderChat([]);
    await refreshChats();
    renderPermissionPrompt();
  });
}

async function selectChat(sessionId) {
  if (!ctx.appId || !sessionId || sessionId === ctx.chatId) return;
  await guard(async () => {
    showWorkspace();
    await invoke("chat_select", { app_id: ctx.appId, session_id: sessionId });
    ctx.chatId = sessionId;
    setBusy(false);
    await loadHistory();
    renderChatList();
    renderPermissionPrompt();
  });
}

async function deleteChat(sessionId) {
  if (!ctx.appId) return;
  const chat = (ctx.chats || []).find((item) => item.id === sessionId);
  const ok = await askConfirm({
    title: "Удалить чат?",
    message: `Диалог «${(chat && chat.title) || sessionId}» будет удалён. Действие необратимо.`,
    confirmLabel: "Удалить",
    danger: true,
  });
  if (!ok) return;
  await guard(async () => {
    const info = await invoke("chat_delete", { app_id: ctx.appId, session_id: sessionId });
    ctx.chatId = info.session;
    const pins = pinnedChatIds().filter((id) => id !== sessionId);
    try {
      localStorage.setItem(pinnedChatsKey(), JSON.stringify(pins));
    } catch (_) {}
    await refreshChats();
    await loadHistory();
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (item) => item.session_id !== sessionId,
    );
    renderPermissionPrompt();
  });
}

const MAX_ATTACHMENTS = 3;
const MAX_IMAGE_BYTES = 4 * 1024 * 1024;
// Must match the backend magic check: anything else is rejected on send.
const ATTACHMENT_IMAGE_MIMES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);
const MAX_TEXT_BYTES = 256 * 1024;
const MAX_DOCUMENT_BYTES = 25 * 1024 * 1024;
const MAX_TOTAL_ATTACHMENT_BYTES = 50 * 1024 * 1024;
const ATTACHMENT_TEXT_MIMES = new Set([
  "application/json",
  "application/xml",
  "application/x-yaml",
  "application/yaml",
  "application/toml",
  "application/javascript",
]);
const ATTACHMENT_DOC_MIMES = new Set([
  "application/pdf",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  "application/vnd.oasis.opendocument.text",
  "application/vnd.oasis.opendocument.spreadsheet",
  "application/vnd.oasis.opendocument.presentation",
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
  pdf: "application/pdf",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  odt: "application/vnd.oasis.opendocument.text",
  ods: "application/vnd.oasis.opendocument.spreadsheet",
  odp: "application/vnd.oasis.opendocument.presentation",
};

function attachmentMime(file) {
  const declared = String(file.type || "").toLowerCase();
  if (declared && declared !== "application/octet-stream") {
    return declared === "image/jpg" ? "image/jpeg" : declared;
  }
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
    if (attachment.mime && attachment.mime.startsWith("image/") && attachment.data) {
      const thumb = document.createElement("img");
      thumb.className = "chat-attachment-thumb";
      thumb.alt = "";
      thumb.src = `data:${attachment.mime};base64,${attachment.data}`;
      chip.appendChild(thumb);
    }
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
  const selected = selectedModelInfo();
  const caps = (selected && selected.model.capabilities) || {};
  let totalBytes = ctx.attachments.reduce(
    (sum, attachment) => sum + Number(attachment.size || 0),
    0,
  );
  for (const file of files) {
    if (ctx.attachments.length >= MAX_ATTACHMENTS) {
      toast(`Не больше ${MAX_ATTACHMENTS} вложений в сообщении`, true);
      break;
    }
    const mime = attachmentMime(file);
    const isImage = mime.startsWith("image/");
    const isText = mime.startsWith("text/") || ATTACHMENT_TEXT_MIMES.has(mime);
    const isDocument = ATTACHMENT_DOC_MIMES.has(mime);
    if (isImage && !ATTACHMENT_IMAGE_MIMES.has(mime)) {
      toast(`Формат изображения не поддерживается: ${file.name}`, true);
      continue;
    }
    if (!isImage && !isText && !isDocument) {
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
    if (isDocument && file.size > MAX_DOCUMENT_BYTES) {
      toast(`Документ больше ${Math.round(MAX_DOCUMENT_BYTES / (1024 * 1024))} МиБ`, true);
      continue;
    }
    if (totalBytes + file.size > MAX_TOTAL_ATTACHMENT_BYTES) {
      toast(
        `Вложения больше ${Math.round(MAX_TOTAL_ATTACHMENT_BYTES / (1024 * 1024))} МиБ в сумме`,
        true,
      );
      break;
    }
    try {
      const data = await readFileBase64(file);
      ctx.attachments.push({
        mime,
        filename: file.name || "attachment",
        data,
        size: file.size,
      });
      totalBytes += file.size;
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

// The composer starts as a single row and grows with the text, up to the CSS
// max-height, so a long message stays readable instead of scrolling sideways.
function autosizeComposer() {
  const textarea = $("chat-text");
  if (!textarea) return;
  textarea.style.height = "auto";
  const max = 180;
  textarea.style.height = `${Math.min(textarea.scrollHeight, max)}px`;
  textarea.style.overflowY = textarea.scrollHeight > max ? "auto" : "hidden";
}

function showEditBanner(show) {
  const banner = $("chat-edit-banner");
  if (banner) banner.hidden = !show;
}

function beginEdit(item) {
  if (!item || !item.message_id) {
    toast("Это сообщение нельзя изменить", true);
    return;
  }
  ctx.editTarget = { id: item.message_id };
  const text = $("chat-text");
  text.value = item.text || "";
  autosizeComposer();
  showEditBanner(true);
  text.focus();
  text.setSelectionRange(text.value.length, text.value.length);
}

function cancelEdit() {
  ctx.editTarget = null;
  showEditBanner(false);
}

/// Reverts the message and everything after it, then reloads history so the
/// optimistic turn does not stack on top of the reverted one.
/// Hides the edited message and everything after it immediately. Opencode
/// keeps reverted messages in `GET /message` until the next prompt is sent, so
/// reloading history there would show the old turn next to the new one.
function truncateAtMessage(messageId) {
  const items = ctx.chatItems || [];
  const index = items.findIndex((item) => item.message_id === messageId);
  if (index < 0) return false;
  ctx.chatItems = items.slice(0, index);
  ctx.streamingItems = [];
  ctx.acceptStream = false;
  renderChat(ctx.chatItems);
  return true;
}

async function revertForResend(messageId) {
  await invoke("chat_revert", { app_id: ctx.appId, message_id: messageId });
  // Fall back to the engine's view only if the id is not in the local list.
  if (!truncateAtMessage(messageId)) await loadHistory();
}

async function regenerateFrom(userItem) {
  if (!ctx.appId || ctx.busy || !userItem || !userItem.message_id) return;
  const text = userItem.text || "";
  if (!text.trim()) {
    toast("Нечего перегенерировать", true);
    return;
  }
  setBusy(true);
  try {
    await revertForResend(userItem.message_id);
  } catch (e) {
    chatError(String(e));
    setBusy(false);
    return;
  }
  await dispatchTurn(text, [], ctx.reasoning || null);
}

async function sendChat() {
  if (!ctx.appId) return;
  const text = $("chat-text").value.trim();
  const attachments = ctx.attachments.map((attachment) => ({
    mime: attachment.mime,
    filename: attachment.filename,
    data: attachment.data,
  }));
  if ((!text && !attachments.length) || ctx.busy) return;
  const edit = ctx.editTarget;
  setBusy(true);
  if (edit) {
    ctx.editTarget = null;
    showEditBanner(false);
    try {
      await revertForResend(edit.id);
    } catch (e) {
      chatError(String(e));
      setBusy(false);
      return;
    }
  }
  $("chat-text").value = "";
  autosizeComposer();
  clearAttachments();
  await dispatchTurn(text, attachments, ctx.reasoning || null);
}

/// Sends one turn and renders the optimistic user message. Shared by the
/// composer and the "Повторить" button on an interrupted turn.
async function dispatchTurn(text, attachments, variant) {
  const appId = ctx.appId;
  try {
    const chat = await ensureChat();
    // The start is tied to the app that was open when the turn was queued.
    // Never send the turn into a different app after a switch.
    if (!chat || ctx.appId !== appId) {
      setBusy(false);
      return;
    }
    const optimistic = [...(ctx.chatItems || [])];
    if (text) optimistic.push({ role: "user", kind: "text", text });
    for (const attachment of attachments) {
      optimistic.push({
        role: "user",
        kind: "file",
        text: attachment.filename,
        mime: attachment.mime,
        data: attachment.data,
      });
    }
    ctx.chatItems = optimistic;
    ctx.streamingItems = [];
    ctx.acceptStream = true;
    renderChat(optimistic);
    const log = $("chat-log");
    if (log) log.scrollTop = log.scrollHeight;
    await invoke("chat_send", { app_id: appId, text, variant, attachments });
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

/// Resends the last user message (with cached images) after an interrupted
/// turn. The user decides to retry; nothing is resent automatically.
async function retryLastTurn() {
  if (!ctx.appId || ctx.busy) return;
  const items = ctx.chatItems || [];
  let start = -1;
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i].role === "user") {
      start = i;
      break;
    }
  }
  if (start < 0) {
    toast("Нет сообщения для повтора", true);
    return;
  }
  let text = "";
  const attachments = [];
  for (let i = start; i < items.length; i++) {
    const item = items[i];
    if (item.role !== "user") continue;
    if (item.kind === "text") {
      text = text ? `${text}\n${item.text}` : item.text;
      continue;
    }
    if (item.kind !== "file" || !item.mime || !item.mime.startsWith("image/")) continue;
    let data = item.data;
    if (!data && item.image_ref) {
      try {
        const image = await invoke("chat_image", {
          app_id: ctx.appId,
          image_ref: item.image_ref,
        });
        data = image && image.data;
      } catch (_) {}
    }
    if (data) attachments.push({ mime: item.mime, filename: item.text, data });
  }
  if (!text.trim() && !attachments.length) {
    toast("Повторите сообщение вручную — вложение больше не доступно", true);
    return;
  }
  setBusy(true);
  await dispatchTurn(text, attachments.slice(0, MAX_ATTACHMENTS), ctx.reasoning || null);
}

async function stopChat() {
  await guard(async () => {
    await invoke("chat_abort", { app_id: ctx.appId });
    toast("Останавливаю текущий ход");
  });
}

// Metadata keys that may carry file bodies; the permission card never renders
// them, even if an engine puts them into the request metadata.
const PERMISSION_CONTENT_KEYS = new Set([
  "content",
  "newString",
  "oldString",
  "diff",
  "patch",
  "text",
  "body",
]);

function permissionForThisChat() {
  return (ctx.pendingPermissions || []).filter(
    (item) =>
      item.app_id === ctx.appId &&
      (!item.session_id || item.session_id === ctx.chatId),
  );
}

function renderPermissionPrompt() {
  const box = $("chat-permission");
  if (!box) return;
  const pending = permissionForThisChat();
  if (!pending.length) {
    box.hidden = true;
    box.innerHTML = "";
    return;
  }
  const item = pending[0];
  const stream = item.stream === "exec" ? "exec" : item.stream === "write" ? "запись" : "инструмент";
  const patterns = (item.patterns || []).filter(Boolean);
  const scope =
    patterns.length && !(patterns.length === 1 && patterns[0] === "*")
      ? patterns.join(", ")
      : "рабочая папка";
  const signed = item.package_exec
    ? "исполняемый инструмент пакета"
    : item.stream === "exec"
      ? "exec из рабочей папки"
      : item.bin
        ? "инструмент пакета"
        : "инструмент платформы";
  const target =
    item.metadata && typeof item.metadata === "object" && typeof item.metadata.target === "string"
      ? item.metadata.target.trim()
      : "";
  const rest = {};
  if (item.metadata && typeof item.metadata === "object") {
    for (const [key, value] of Object.entries(item.metadata)) {
      if (key === "target" || PERMISSION_CONTENT_KEYS.has(key)) continue;
      rest[key] = value;
    }
  }
  const meta = Object.keys(rest).length ? JSON.stringify(rest) : "";
  box.hidden = false;
  box.innerHTML = `
    <div class="chat-permission-head">
      <strong>${esc(item.tool || item.action)}</strong>
      <span class="badge warn">${esc(stream)}</span>
      ${pending.length > 1 ? `<span class="badge">ещё ${pending.length - 1}</span>` : ""}
    </div>
    ${target ? `<div class="chat-permission-target"><code>${esc(target)}</code></div>` : ""}
    <div class="muted small">${esc(signed)} · ${esc(scope)}</div>
    ${meta ? `<div class="mono small muted" style="margin-top:4px;white-space:pre-wrap;overflow-wrap:anywhere">${esc(meta.slice(0, 400))}</div>` : ""}
    <div class="row-actions chat-permission-actions">
      <button class="ghost small" type="button" data-permission="reject">Отклонить</button>
      <button class="ghost small" type="button" data-permission="session" ${item.can_session ? "" : "disabled"}>В этом чате</button>
      <button class="ghost small" type="button" data-permission="always" ${item.can_always ? "" : "disabled"}>Всегда для пакета</button>
      <button class="small" type="button" data-permission="once">Разрешить</button>
    </div>`;
}

async function resolvePermission(decision) {
  const pending = permissionForThisChat();
  const item = pending[0];
  if (!item) return;
  await guard(async () => {
    await invoke("chat_permission", {
      app_id: item.app_id,
      session_id: item.session_id,
      permission_id: item.id,
      decision,
    });
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (entry) => entry.id !== item.id,
    );
    renderPermissionPrompt();
  });
}

function bindPermissionUi() {
  const box = $("chat-permission");
  if (!box) return;
  box.addEventListener("click", (event) => {
    const button = event.target.closest("[data-permission]");
    if (!button || button.disabled) return;
    resolvePermission(button.dataset.permission);
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
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (item) => item.session_id !== ctx.chatId,
    );
    renderPermissionPrompt();
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
      ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
        (item) => item.session_id !== ctx.chatId,
      );
      renderPermissionPrompt();
      if (ctx.memory && ctx.memory.pendingRestart) {
        ctx.memory.pendingRestart = false;
        // The backend restarted the engine after this turn.
        ctx.chat = null;
      }
      refreshChats();
      guard(maybeAutoCompact);
    }
  });
  listen("chat://permission", (event) => {
    const payload = event.payload || {};
    if (!payload.id) return;
    const list = ctx.pendingPermissions || [];
    const index = list.findIndex((item) => item.id === payload.id);
    // A repeated id is the broker refreshing the same card (the tool part
    // arrived after the ask and brought the target), not a duplicate.
    ctx.pendingPermissions =
      index >= 0
        ? list.map((item, at) => (at === index ? payload : item))
        : [...list, payload];
    renderPermissionPrompt();
  });
  listen("chat://permission_resolved", (event) => {
    const payload = event.payload || {};
    if (payload.app_id && payload.app_id !== ctx.appId) return;
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (item) => item.id !== payload.permission_id,
    );
    renderPermissionPrompt();
  });
  listen("chat://error", (event) => {
    const payload = event.payload || {};
    if (payload.app_id !== ctx.appId) return;
    if (payload.session && payload.session !== ctx.chatId) return;
    ctx.acceptStream = false;
    // Keep what the user already saw: thoughts and tool steps that streamed
    // before the failure are a real part of the turn (the engine persists
    // them, they reappear after a restart). Commit the last live snapshot
    // instead of discarding it.
    if ((ctx.streamingItems || []).length) {
      ctx.chatItems = [...(ctx.chatItems || []), ...ctx.streamingItems];
      renderChat(ctx.chatItems);
    }
    ctx.streamingItems = [];
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (item) => item.session_id !== ctx.chatId,
    );
    renderPermissionPrompt();
    if (payload.interrupted) {
      chatInterrupted(payload.message || "движок перестал отвечать");
    } else {
      chatError(payload.message || "ошибка движка");
    }
    setBusy(false);
  });
  listen("chat://retry", (event) => {
    const payload = event.payload || {};
    if (payload.app_id !== ctx.appId) return;
    if (payload.session && payload.session !== ctx.chatId) return;
    const seconds = Math.max(1, Math.round(Number(payload.delay_ms || 0) / 1000));
    toast(
      `Сбой шлюза/модели, повторяю ${payload.attempt}/${payload.max_attempts} через ${seconds} с…`,
    );
  });
}

/* ---------- mcp ---------- */

async function refreshMcpServers() {
  ctx.mcp = await invoke("mcp_servers");
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

/* ---------- per-app settings ---------- */

function scopeBadge(scope) {
  if (scope === "app") return '<span class="badge ok">override приложения</span>';
  if (scope === "global") return '<span class="badge">глобальный</span>';
  return '<span class="badge bad">не настроен</span>';
}

async function refreshAppSettings() {
  if (!ctx.appId) return;
  const appId = ctx.appId;
  const [servers, secrets] = await Promise.all([
    invoke("mcp_app_servers", { app_id: appId }),
    invoke("app_secrets", { app_id: appId }),
  ]);
  if (ctx.appId !== appId) return;
  ctx.appMcp = servers || [];
  ctx.appSecrets = secrets || [];
  renderAppMcp();
  renderAppSecrets();
  const appPolicy =
    ctx.config &&
    ctx.config.permissions &&
    ctx.config.permissions.apps &&
    ctx.config.permissions.apps[appId];
  $("app-perm-write").value = (appPolicy && appPolicy.write) || "inherit";
  $("app-perm-exec").value = (appPolicy && appPolicy.exec) || "inherit";
  await refreshPermissionGrants();
  const auditLines = await invoke("audit_tail", { app_id: appId, limit: 40 }).catch(() => []);
  renderPermissionAudit(auditLines);
  await refreshAppMemory();
}

function formatMemoryBytes(bytes) {
  const value = Number(bytes || 0);
  if (value < 1024) return `${value} Б`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} КБ`;
  return `${(value / (1024 * 1024)).toFixed(1)} МБ`;
}

function formatMemoryDate(ts) {
  if (!ts) return "—";
  try {
    return new Date(Number(ts) * 1000).toLocaleString("ru-RU", {
      day: "2-digit",
      month: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch (_) {
    return "—";
  }
}

async function refreshAppMemory() {
  if (!ctx.appId) return;
  const appId = ctx.appId;
  const status = await invoke("memory_status", { app_id: appId }).catch(() => null);
  if (!status || ctx.appId !== appId) return;
  const previousQuery =
    ctx.memory && ctx.memory.appId === appId ? ctx.memory.query || "" : "";
  ctx.memory = {
    ...status,
    appId,
    query: previousQuery,
    facts: [],
    offset: 0,
    total: 0,
    has_more: false,
  };
  const search = $("app-memory-search");
  if (search) search.value = previousQuery;
  await loadAppMemory(true);
}

async function loadAppMemory(reset) {
  const memory = ctx.memory;
  if (!memory || !ctx.appId) return;
  if (reset) {
    memory.facts = [];
    memory.offset = 0;
  }
  const page = await invoke("memory_list", {
    app_id: ctx.appId,
    query: memory.query || "",
    offset: memory.offset || 0,
    limit: 50,
  });
  if (!ctx.memory || ctx.memory.appId !== ctx.appId) return;
  ctx.memory.facts = reset
    ? page.facts || []
    : [...(ctx.memory.facts || []), ...(page.facts || [])];
  ctx.memory.offset = Number(page.offset || 0) + (page.facts || []).length;
  ctx.memory.total = page.total || 0;
  ctx.memory.has_more = Boolean(page.has_more);
  renderAppMemory();
}

function renderAppMemory() {
  const memory = ctx.memory;
  const list = $("app-memory-list");
  if (!memory || !list) return;
  const toggle = $("app-memory-toggle");
  if (toggle) toggle.textContent = memory.enabled ? "Выключить" : "Включить";
  const status = $("app-memory-status");
  if (status) {
    status.textContent = `${memory.enabled ? "включена" : "выключена"} · ${
      memory.workspace || ""
    } · ${memory.count || 0} фактов · ${formatMemoryBytes(memory.bytes)}`;
  }
  list.innerHTML = "";
  const facts = memory.facts || [];
  const empty = $("app-memory-empty");
  if (empty) {
    empty.hidden = facts.length > 0;
    empty.textContent = memory.query
      ? "По запросу ничего не найдено."
      : memory.enabled
        ? "Пока нет сохранённых фактов."
        : "Память выключена: агент не видит инструменты памяти. Факты сохранены.";
  }
  const clear = $("app-memory-clear");
  if (clear) clear.disabled = !(memory.count > 0);
  const more = $("app-memory-more");
  if (more) more.hidden = !memory.has_more;

  for (const fact of facts) {
    const card = document.createElement("div");
    card.className = "app-setting-card memory-fact";
    const head = document.createElement("div");
    head.className = "memory-fact-head";
    const date = document.createElement("span");
    date.textContent = formatMemoryDate(fact.ts);
    const id = document.createElement("span");
    id.className = "mono";
    id.textContent = fact.id;
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "ghost small memory-fact-actions";
    remove.textContent = "Удалить";
    remove.addEventListener("click", () =>
      guard(async () => {
        const ok = await askConfirm({
          title: "Удалить факт?",
          message: fact.text,
          confirmLabel: "Удалить",
          danger: true,
        });
        if (!ok) return;
        await invoke("memory_forget", { app_id: ctx.appId, id: fact.id });
        toast("Факт удалён");
        await refreshAppMemory();
      }),
    );
    head.append(date, id, remove);
    const text = document.createElement("div");
    text.className = "memory-fact-text";
    text.textContent = fact.text;
    card.append(head, text);
    if (fact.keywords && fact.keywords.length) {
      const keywords = document.createElement("div");
      keywords.className = "memory-fact-keywords";
      keywords.textContent = fact.keywords.join(" · ");
      card.append(keywords);
    }
    list.append(card);
  }
}

function renderAppMcp() {
  const box = $("app-mcp-list");
  if (!box) return;
  box.innerHTML = "";
  if (!(ctx.appMcp || []).length) {
    box.innerHTML = '<span class="muted small">В манифесте нет MCP-подключений.</span>';
    return;
  }
  for (const server of ctx.appMcp) {
    const card = document.createElement("div");
    card.className = "app-setting-card";
    card.dataset.appMcp = server.id;
    card.innerHTML = `
      <div class="app-setting-head">
        <strong class="mono">${esc(server.id)}</strong>
        ${scopeBadge(server.scope)}
      </div>
      <div class="grid">
        <label>Endpoint URL<input data-field="url" placeholder="${esc(server.url || "https://…")}" /></label>
        <label>Bearer-токен<input data-field="token" type="password" placeholder="${server.has_token ? "сохранить текущий" : "не задан"}" /></label>
      </div>
      <label class="check-row danger-check">
        <input data-field="insecure" type="checkbox" ${server.allow_invalid_certs ? "checked" : ""} />
        <span>Не проверять TLS-сертификат и имя хоста</span>
      </label>
      <div class="row-actions">
        <button class="small" type="button" data-app-mcp-save="${esc(server.id)}">Сохранить</button>
        <button class="ghost small" type="button" data-app-mcp-probe="${esc(server.id)}">Проверить tools</button>
        <button class="ghost small danger-hover" type="button" data-app-mcp-reset="${esc(server.id)}" ${server.has_app_override ? "" : "disabled"}>Сбросить override</button>
      </div>`;
    box.appendChild(card);
  }
}

function renderAppSecrets() {
  const box = $("app-secrets-list");
  if (!box) return;
  box.innerHTML = "";
  if (!(ctx.appSecrets || []).length) {
    box.innerHTML = '<span class="muted small">Пакет не объявляет переменных инструментов.</span>';
    return;
  }
  for (const secret of ctx.appSecrets) {
    const card = document.createElement("div");
    card.className = "app-setting-card";
    card.dataset.appSecret = secret.name;
    const envs = (secret.envs || []).map((env) => `<code>${esc(env)}</code>`).join(", ");
    card.innerHTML = `
      <div class="app-setting-head">
        <strong class="mono">${esc(secret.name)}</strong>
        ${scopeBadge(secret.scope)}
      </div>
      <div class="muted small">Переменные: ${envs || "—"}</div>
      <div class="grid">
        <label>Новое значение<input data-field="value" type="password" placeholder="значение в DPAPI" /></label>
      </div>
      <div class="row-actions">
        <button class="small" type="button" data-app-secret-save="${esc(secret.name)}">Сохранить</button>
        <button class="ghost small danger-hover" type="button" data-app-secret-reset="${esc(secret.name)}" ${secret.scope === "app" ? "" : "disabled"}>Удалить override</button>
      </div>`;
    box.appendChild(card);
  }
}

async function saveAppMcp(id) {
  const card = document.querySelector(`.app-setting-card[data-app-mcp="${CSS.escape(id)}"]`);
  if (!card) return;
  const url = card.querySelector('[data-field="url"]').value.trim();
  const token = card.querySelector('[data-field="token"]').value;
  const allowInvalid = card.querySelector('[data-field="insecure"]').checked;
  await guard(async () => {
    await invoke("mcp_app_server_save", {
      app_id: ctx.appId,
      id,
      url,
      token: token === "" ? null : token,
      allow_invalid_certs: allowInvalid,
    });
    toast(`MCP ${id}: настройки приложения сохранены`);
    await refreshAppSettings();
  });
}

async function resetAppMcp(id) {
  await guard(async () => {
    await invoke("mcp_app_server_delete", { app_id: ctx.appId, id });
    toast(`MCP ${id}: override удалён, используется глобальный`);
    await refreshAppSettings();
  });
}

async function probeAppMcp(id) {
  await guard(async () => {
    const r = await invoke("mcp_app_probe", { app_id: ctx.appId, id });
    const tools = (r.tools || []).join(", ");
    toast(`${(r.server && r.server.name) || id}: ${tools || "инструменты не найдены"}`);
  });
}

async function saveAppSecret(name) {
  const card = document.querySelector(`.app-setting-card[data-app-secret="${CSS.escape(name)}"]`);
  if (!card) return;
  const value = card.querySelector('[data-field="value"]').value;
  if (!value) return toast("Введите значение", true);
  await guard(async () => {
    await invoke("app_secret_set", { app_id: ctx.appId, name, value });
    toast(`Секрет ${name}: значение приложения сохранено`);
    await refreshAppSettings();
  });
}

async function resetAppSecret(name) {
  await guard(async () => {
    await invoke("app_secret_delete", { app_id: ctx.appId, name });
    toast(`Секрет ${name}: override удалён`);
    await refreshAppSettings();
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
let artifactsLastRefresh = 0;

async function refreshArtifacts() {
  artifactsLastRefresh = Date.now();
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
  const ok = await askConfirm({
    title: "Очистить дампы?",
    message: `Будут удалены завершённые сессии артефактов приложения ${appId}. Активные сессии останутся.`,
    confirmLabel: "Очистить",
    danger: true,
  });
  if (!ok) return;
  await guard(async () => {
    const report = await invoke("cleanup_artifacts", { app_id: appId });
    toast(`Удалено сессий: ${report.removed} · ${fmtBytes(report.bytes_freed)}`);
    await refreshArtifacts();
  });
}

/* ---------- package wizard ---------- */

function renderPackStatus(info) {
  const badge = $("pack-status");
  if (info.available) {
    badge.textContent = info.version || "eco-pack готов";
    badge.className = "badge mono ok";
    badge.title = info.path || "";
  } else {
    badge.textContent = "eco-pack: не найден";
    badge.className = "badge mono bad";
    badge.title = "Соберите tools\\build-pack.ps1 или задайте ECO_PACK_BIN";
  }
  if (info.default_key && !$("pack-key").value) {
    $("pack-key").value = info.default_key;
  }
}

async function refreshPackTool(force) {
  if (ctx.packInfo && !force) {
    renderPackStatus(ctx.packInfo);
    return;
  }
  const info = await invoke("pack_tool_info");
  ctx.packInfo = info;
  renderPackStatus(info);
}

function packLog(text, ok) {
  const box = $("pack-log");
  box.hidden = false;
  box.textContent = text || "";
  box.className = "pack-log" + (ok === false ? " error" : "");
}

function packDirValue() {
  return ($("pack-dir").value || ctx.packDir || "").trim();
}

function renderPackFiles() {
  const box = $("pack-files");
  box.innerHTML = "";
  if (!(ctx.packFiles || []).length) {
    box.innerHTML = '<span class="muted small">файлов нет; создайте каркас</span>';
    return;
  }
  for (const file of ctx.packFiles) {
    const row = document.createElement("div");
    row.className = "pack-file" + (file.path === ctx.packFile ? " active" : "");
    row.dataset.packFile = file.path;
    row.innerHTML = `<span>${esc(file.path)}</span><span class="muted small">${file.bytes} Б</span>`;
    box.appendChild(row);
  }
}

async function renderPackMcp() {
  const box = $("pack-mcp");
  if (!box) return;
  box.innerHTML = "";
  const manifest = ctx.packManifest;
  if (!manifest) {
    box.innerHTML = '<span class="muted small">сначала загрузите или создайте манифест</span>';
    return;
  }
  let servers = [];
  try {
    servers = await invoke("mcp_servers");
  } catch (_) {
    servers = [];
  }
  const connections = (manifest.connections || []).filter(
    (connection) => connection.type === "http-mcp"
  );
  const writes = (manifest.capabilities && manifest.capabilities.external_writes) || [];
  if (!connections.length) {
    box.innerHTML =
      '<span class="muted small">MCP не объявлены; endpoint и токен задаются в настройках приложения</span>';
    return;
  }
  for (const connection of connections) {
    const known = servers.includes(connection.id);
    const row = document.createElement("div");
    row.className = "pack-file";
    row.innerHTML = `
      <span>${esc(connection.id)}</span>
      <span class="badge ${known ? "ok" : "warn"}">${known ? "сервер настроен" : "нет в настройках"}</span>
      <label class="check-row"><input type="checkbox" data-mcp-write="${esc(connection.id)}" ${writes.includes(connection.id) ? "checked" : ""}/><span>запись</span></label>`;
    box.appendChild(row);
  }
}

async function addPackMcp() {
  const dir = packDirValue();
  const id = $("pack-mcp-id").value.trim();
  if (!dir) return toast("Укажите папку пакета", true);
  if (!id) return toast("Укажите id MCP-подключения", true);
  await guard(async () => {
    const result = await invoke("pack_set_mcp", {
      dir,
      id,
      write: $("pack-mcp-write").checked,
    });
    if (!result.ok) {
      packLog(result.output, false);
      return toast("Не удалось добавить MCP", true);
    }
    $("pack-mcp-id").value = "";
    $("pack-mcp-write").checked = false;
    await loadPack();
    packLog(result.output, true);
    toast("MCP-подключение добавлено");
  });
}

async function selectPackFile(path, silent) {
  const dir = packDirValue();
  if (!dir) return;
  await guard(async () => {
    const text = await invoke("pack_read_text", { dir, path });
    ctx.packFile = path;
    $("pack-editor").value = text;
    $("pack-editor-path").textContent = path;
    renderPackFiles();
    if (!silent) packLog(`Открыт ${path}`, true);
  });
}

async function loadPack() {
  const dir = packDirValue();
  if (!dir) return toast("Укажите папку пакета", true);
  await guard(async () => {
    const [files, scan] = await Promise.all([
      invoke("pack_files", { dir }),
      invoke("pack_scan", { dir }),
    ]);
    ctx.packDir = dir;
    ctx.packFiles = files;
    ctx.packScan = scan;
    renderPackFiles();

    const folder = dir.split(/[\\/]/).filter(Boolean).pop() || "";
    const slug = folder
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/^-+|-+$/g, "");
    if (slug && !$("pack-app-id").value) $("pack-app-id").value = slug;
    if (folder && !$("pack-name").value) $("pack-name").value = folder;

    if (files.some((file) => file.path === "manifest.json")) {
      await selectPackFile("manifest.json", true);
      try {
        const manifest = JSON.parse($("pack-editor").value);
        ctx.packManifest = manifest;
        if (manifest.app_id) $("pack-app-id").value = manifest.app_id;
        if (manifest.name) $("pack-name").value = manifest.name;
      } catch (_) {
        ctx.packManifest = null;
      }
    } else {
      ctx.packFile = null;
      ctx.packManifest = null;
      $("pack-editor").value = "";
      $("pack-editor-path").textContent = "";
    }
    await renderPackMcp();

    const parts = [
      `AGENTS.md: ${scan.agents_md ? "найден" : "нет"}`,
      `навыков: ${scan.skills.length}${scan.root_skill ? " + корневой SKILL.md" : ""}`,
      `bin: ${scan.bins.length}`,
      `MCP: ${scan.mcp.length}`,
      `секреты (.env): ${scan.env_files.length}`,
      `manifest.json: ${scan.has_manifest ? "есть" : "нет"}`,
    ];
    if (!scan.has_manifest) {
      parts.push(
        "Манифеста нет. Проверьте app_id/название и нажмите «Создать манифест из исходников»."
      );
    }
    packLog(parts.join("\n"), scan.has_manifest);
  });
}

async function importPack() {
  const dir = packDirValue();
  const appId = $("pack-app-id").value.trim();
  if (!dir) return toast("Укажите папку пакета", true);
  if (!appId) return toast("Укажите app_id", true);
  await guard(async () => {
    const result = await invoke("pack_import", {
      dir,
      app_id: appId,
      name: $("pack-name").value.trim() || appId,
      template: $("pack-template").value,
    });
    packLog(result.output, result.ok);
    if (!result.ok) return toast("Не удалось создать манифест", true);
    await loadPack();
    packLog(
      `${result.output}\n\nСледующий шаг — «Собрать zip»: он пересчитает hashes и создаст архив.`,
      true
    );
    toast("Манифест создан");
  });
}

async function pickPackFolder() {
  await guard(async () => {
    const dir = await invoke("pack_pick_folder", { title: "Папка мини-приложения" });
    if (!dir) return;
    $("pack-dir").value = dir;
    await loadPack();
  });
}

async function initPack() {
  const dir = packDirValue();
  const appId = $("pack-app-id").value.trim();
  if (!dir || !appId) return toast("Папка и app_id обязательны", true);
  await guard(async () => {
    const result = await invoke("pack_init", {
      dir,
      app_id: appId,
      name: $("pack-name").value.trim() || appId,
      template: $("pack-template").value,
    });
    packLog(result.output, result.ok);
    if (!result.ok) return toast("eco-pack init завершился с ошибкой", true);
    await loadPack();
    toast("Каркас создан");
  });
}

async function runPackAction(action) {
  const dir = packDirValue();
  if (!dir) return toast("Укажите папку пакета", true);
  const args = { dir };
  if (action === "sign") {
    args.key = $("pack-key").value.trim();
    if (!args.key) return toast("Укажите dev-ключ", true);
  }
  await guard(async () => {
    const result = await invoke(`pack_${action}`, args);
    packLog(result.output, result.ok);
    if (result.ok) {
      toast(action === "build" ? "Архив собран. Следующий шаг — «Подписать»" : "Готово");
      return;
    }
    if (action === "validate" && /not covered by hashes/i.test(result.output)) {
      toast("Сначала «Собрать zip»: он пересчитает hashes", true);
      return;
    }
    toast("eco-pack вернул ошибку", true);
  });
}

async function savePackFile() {
  const dir = packDirValue();
  const path = ctx.packFile;
  if (!dir || !path) return toast("Файл не выбран", true);
  await guard(async () => {
    await invoke("pack_write_text", { dir, path, text: $("pack-editor").value });
    packLog(`Сохранён ${path}`, true);
    ctx.packFiles = await invoke("pack_files", { dir });
    renderPackFiles();
  });
}

async function createPackFile() {
  const dir = packDirValue();
  const path = $("pack-new-file").value.trim();
  if (!dir) return toast("Укажите папку пакета", true);
  if (!path) return toast("Укажите путь файла", true);
  await guard(async () => {
    await invoke("pack_write_text", { dir, path, text: "" });
    $("pack-new-file").value = "";
    ctx.packFiles = await invoke("pack_files", { dir });
    await selectPackFile(path, true);
    packLog(`Создан ${path}`, true);
  });
}

async function keygenPack() {
  await guard(async () => {
    const out = await invoke("pack_pick_folder", { title: "Папка для dev-ключа" });
    if (!out) return;
    const result = await invoke("pack_keygen", { out_dir: out });
    packLog(result.output, result.ok);
    if (!result.ok) return toast("Не удалось создать ключ", true);
    $("pack-key").value = `${out.replace(/[\\/]+$/, "")}\\dev.key`;
    toast("Dev-ключ создан");
  });
}

async function installPack() {
  const dir = packDirValue();
  if (!dir) return toast("Укажите папку пакета", true);
  await guard(async () => {
    const app = await invoke("install_app", { path: dir });
    toast(`Установлено: ${app.name} ${app.version}`);
    await refreshApps();
  });
}

async function applyPackTemplate() {
  const dir = packDirValue();
  const template = $("pack-template").value;
  if (!dir) return toast("Укажите папку пакета", true);
  await guard(async () => {
    const result = await invoke("pack_apply_template", { dir, template });
    packLog(result.output, result.ok);
    if (!result.ok) return toast("Не удалось применить шаблон", true);
    await loadPack();
    packLog(
      `${result.output}\n\nПересоберите zip и подпишите пакет заново, затем установите.`,
      true
    );
    toast("Шаблон применён");
  });
}

/* ---------- LLM providers ---------- */

function capabilitiesOrDefault(caps) {
  return {
    vision: !!(caps && caps.vision),
    reasoning: !caps || caps.reasoning !== false,
    reasoning_efforts:
      caps && Array.isArray(caps.reasoning_efforts) && caps.reasoning_efforts.length
        ? [...caps.reasoning_efforts]
        : ["low", "medium", "high"],
    context_tokens: (caps && caps.context_tokens) || null,
    output_tokens: (caps && caps.output_tokens) || null,
  };
}

function pricingOrDefault(pricing) {
  const value = pricing || {};
  const num = (input) => {
    if (input === null || input === undefined || input === "") return null;
    const parsed = Number(input);
    return Number.isFinite(parsed) ? parsed : null;
  };
  return {
    input_per_mtok: num(value.input_per_mtok),
    output_per_mtok: num(value.output_per_mtok),
    cache_read_per_mtok: num(value.cache_read_per_mtok),
    cache_write_per_mtok: num(value.cache_write_per_mtok),
    currency: String(value.currency || ""),
  };
}

function providersFromConfig(cfg) {
  const config = cfg || {};
  if (Array.isArray(config.llm_providers) && config.llm_providers.length) {
    return config.llm_providers.map((provider) => ({
      id: String(provider.id || ""),
      name: String(provider.name || ""),
      endpoint: String(provider.endpoint || ""),
      allow_invalid_certs: !!provider.allow_invalid_certs,
      models: (provider.models || []).map((model) => ({
        id: String(model.id || ""),
        name: String(model.name || ""),
        capabilities: capabilitiesOrDefault(model.capabilities),
        pricing: pricingOrDefault(model.pricing),
      })),
    }));
  }
  if (!String(config.llm_endpoint || "").trim()) return [];
  return [
    {
      id: "gw",
      name: "LLM Gateway",
      endpoint: String(config.llm_endpoint),
      allow_invalid_certs: !!config.llm_allow_invalid_certs,
      models: [
        {
          id: String(config.llm_model || "stub-model"),
          name: "",
          capabilities: capabilitiesOrDefault(config.llm_capabilities),
          pricing: pricingOrDefault({}),
        },
      ],
    },
  ];
}

function effectiveProviders() {
  return providersFromConfig(ctx.config);
}

function cloneProviders(providers) {
  return JSON.parse(JSON.stringify(providers || []));
}

function resolveModelInfo(providers, raw) {
  if (!providers.length) return null;
  const slash = String(raw || "").indexOf("/");
  const providerId = slash > 0 ? raw.slice(0, slash) : "";
  const modelId = slash > 0 ? raw.slice(slash + 1) : "";
  let provider = providers.find((item) => item.id === providerId);
  let model = provider && provider.models.find((item) => item.id === modelId);
  if (!provider || !model) {
    provider = providers[0];
    model = provider && provider.models[0];
  }
  if (!model) return null;
  return {
    provider,
    model,
    value: `${provider.id}/${model.id}`,
  };
}

function selectedModelInfo() {
  const providers = effectiveProviders();
  const override =
    (ctx.config && ctx.config.llm_models && ctx.config.llm_models[ctx.appId]) || "";
  const fallback = (ctx.config && ctx.config.llm_default) || "";
  return resolveModelInfo(providers, override || fallback);
}

function contextLabel(caps) {
  const tokens = Number(caps && caps.context_tokens);
  return tokens > 0 ? `${Math.round(tokens / 1000)}k` : "контекст не задан";
}

function renderModelChip(placeholder) {
  const label = $("chat-model-label");
  const button = $("chat-model-btn");
  const info = selectedModelInfo();
  if (label) {
    label.textContent = info
      ? info.model.name || info.model.id
      : placeholder || "движок не запущен";
  }
  if (button) {
    button.title = info
      ? `${info.provider.name || info.provider.id} · ${info.model.id}`
      : "Модель и шлюз";
  }
  renderModelMenu();
}

function renderModelMenu() {
  const list = $("chat-model-list");
  if (!list) return;
  const selected = selectedModelInfo();
  list.innerHTML = "";
  const providers = effectiveProviders();
  if (!providers.length) {
    const empty = document.createElement("div");
    empty.className = "chat-pop-empty";
    empty.textContent = "Модели не настроены";
    list.appendChild(empty);
    return;
  }
  for (const provider of providers) {
    if (!provider.models.length) continue;
    const head = document.createElement("div");
    head.className = "chat-pop-provider";
    head.textContent = provider.name || provider.id;
    list.appendChild(head);
    for (const model of provider.models) {
      const value = `${provider.id}/${model.id}`;
      const item = document.createElement("button");
      item.type = "button";
      item.className = "chat-pop-item" + (selected && selected.value === value ? " sel" : "");
      item.dataset.model = value;
      item.setAttribute("role", "menuitem");
      item.innerHTML =
        `<span class="chat-pop-title">${esc(model.name || model.id)}</span>` +
        `<em>${esc(provider.id)} · ${esc(contextLabel(model.capabilities))}</em>`;
      list.appendChild(item);
    }
  }
}

function closeChatPops(except) {
  for (const id of ["chat-context-popover", "chat-model-pop", "chat-reasoning-pop"]) {
    if (id === except) continue;
    const pop = $(id);
    if (pop) pop.hidden = true;
  }
  for (const id of ["chat-context", "chat-model-btn", "chat-reasoning-btn"]) {
    if (id === except) continue;
    const chip = $(id);
    if (chip) chip.setAttribute("aria-expanded", "false");
  }
}

function placeChatPop(pop, trigger, alignRight) {
  pop.hidden = false;
  pop.style.left = "0px";
  pop.style.top = "0px";
  const rect = trigger.getBoundingClientRect();
  const width = pop.offsetWidth;
  const height = pop.offsetHeight;
  let left = alignRight ? rect.right - width : rect.left;
  left = Math.max(12, Math.min(left, window.innerWidth - width - 12));
  let top = rect.top - height - 6;
  if (top < 12) top = rect.bottom + 6;
  top = Math.max(12, Math.min(top, window.innerHeight - height - 12));
  pop.style.left = `${Math.round(left)}px`;
  pop.style.top = `${Math.round(top)}px`;
}

function openModelMenu() {
  const pop = $("chat-model-pop");
  const button = $("chat-model-btn");
  if (!pop || !button) return;
  renderModelMenu();
  closeChatPops("chat-model-pop");
  placeChatPop(pop, button, true);
  button.setAttribute("aria-expanded", "true");
}

function closeModelMenu() {
  const pop = $("chat-model-pop");
  const button = $("chat-model-btn");
  if (pop) pop.hidden = true;
  if (button) button.setAttribute("aria-expanded", "false");
}

async function selectModel(value) {
  closeModelMenu();
  await invoke("llm_select", { app_id: ctx.appId || null, model: value });
  ctx.config = await invoke("get_config");
  renderModelChip();
  renderReasoningOptions();
  updateContextMeter(ctx.chatItems || []);
  const info = selectedModelInfo();
  toast(`Модель: ${info ? info.model.name || info.model.id : "сброшена"}`);
}

/* ---------- settings & secrets ---------- */

const EFFORT_LABELS = {
  low: "Низкий (low)",
  medium: "Средний (medium)",
  high: "Глубокий (high)",
  max: "Максимальный (max)",
};

function reasoningCapabilities() {
  const selected = selectedModelInfo();
  const caps = (selected && selected.model.capabilities) || {};
  return {
    enabled: caps.reasoning !== false,
    efforts:
      Array.isArray(caps.reasoning_efforts) && caps.reasoning_efforts.length
        ? caps.reasoning_efforts
        : ["low", "medium", "high"],
  };
}

function renderReasoningMenu(efforts) {
  const list = $("chat-reasoning-list");
  if (!list) return;
  list.innerHTML = "";
  const options = [
    ["", "Авто"],
    ...efforts.map((effort) => [effort, EFFORT_LABELS[effort] || effort]),
  ];
  for (const [value, text] of options) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "chat-pop-item" + ((ctx.reasoning || "") === value ? " sel" : "");
    item.dataset.effort = value;
    item.setAttribute("role", "menuitem");
    item.textContent = text;
    list.appendChild(item);
  }
}

function renderReasoningOptions() {
  const button = $("chat-reasoning-btn");
  const label = $("chat-reasoning-label");
  if (!button) return;
  const { enabled, efforts } = reasoningCapabilities();
  if (ctx.reasoning && (!enabled || !efforts.includes(ctx.reasoning))) {
    ctx.reasoning = "";
  }
  button.hidden = !enabled;
  if (label) {
    label.textContent = ctx.reasoning
      ? EFFORT_LABELS[ctx.reasoning] || ctx.reasoning
      : "Авто";
  }
  renderReasoningMenu(enabled ? efforts : []);
}

function openReasoningMenu() {
  const pop = $("chat-reasoning-pop");
  const button = $("chat-reasoning-btn");
  if (!pop || !button || button.hidden) return;
  renderReasoningOptions();
  closeChatPops("chat-reasoning-pop");
  placeChatPop(pop, button, false);
  button.setAttribute("aria-expanded", "true");
}

function closeReasoningMenu() {
  const pop = $("chat-reasoning-pop");
  const button = $("chat-reasoning-btn");
  if (pop) pop.hidden = true;
  if (button) button.setAttribute("aria-expanded", "false");
}

function chooseReasoning(value) {
  ctx.reasoning = value || "";
  try {
    localStorage.setItem(`reasoning:${ctx.appId}`, ctx.reasoning);
  } catch (_) {}
  renderReasoningOptions();
  closeReasoningMenu();
  toast(
    ctx.reasoning
      ? `Уровень размышлений: ${EFFORT_LABELS[ctx.reasoning] || ctx.reasoning}`
      : "Уровень размышлений: авто",
  );
}

function renderPermissionGrants(boxId, grants, emptyText) {
  const box = $(boxId);
  if (!box) return;
  if (!(grants || []).length) {
    box.innerHTML = `<span class="muted small">${esc(emptyText)}</span>`;
    return;
  }
  box.innerHTML = grants
    .map(
      (grant) => `
      <div class="app-setting-card">
        <div class="app-setting-head">
          <strong class="mono">${esc(grant.tool)}</strong>
          <span class="badge">${esc(grant.stream === "exec" ? "exec" : "запись")}</span>
        </div>
        <p class="muted small">${esc(grant.app_id)} · ${grant.bin_sha256 ? "инструмент пакета (хеш проверен)" : "рабочая папка"} · v${esc(grant.app_version)} · ${esc(grant.workspace)}</p>
        <div class="row-actions">
          <button class="ghost small danger-hover" type="button" data-grant-revoke="${esc(grant.id)}">Отозвать</button>
        </div>
      </div>`,
    )
    .join("");
}

async function refreshPermissionGrants() {
  const appId = ctx.appId;
  let all = [];
  let scoped = [];
  try {
    [all, scoped] = await Promise.all([
      invoke("permission_grants", { app_id: null }),
      appId ? invoke("permission_grants", { app_id: appId }) : Promise.resolve([]),
    ]);
  } catch (_) {
    all = [];
    scoped = [];
  }
  renderPermissionGrants("perm-grants", all, "Постоянных разрешений нет.");
  if (appId) {
    renderPermissionGrants("app-perm-grants", scoped, "Для этого пакета постоянных разрешений нет.");
  }
}

function renderPermissionAudit(lines) {
  const box = $("app-perm-audit");
  if (!box) return;
  const decisions = {
    allow_once: "разрешено",
    allow_session: "разрешено на чат",
    allow_always: "разрешено всегда",
    auto_grant: "авто по гранту",
    auto_allow: "авто по политике",
    reject: "отклонено",
    reject_cascade: "отклонено (каскад)",
    timeout: "таймаут",
    revoke_grant: "грант отозван",
    revoke_app: "гранты пакета отозваны",
  };
  const items = [];
  for (const line of lines || []) {
    let entry;
    try {
      entry = JSON.parse(line);
    } catch (_) {
      continue;
    }
    if (entry.type !== "permission") continue;
    items.push(entry);
  }
  if (!items.length) {
    box.innerHTML = '<span class="muted small">Решений пока нет.</span>';
    return;
  }
  box.innerHTML = items
    .slice(-8)
    .reverse()
    .map((entry) => {
      const when = entry.ts ? new Date(entry.ts * 1000).toLocaleTimeString("ru-RU") : "";
      const label = decisions[entry.decision] || entry.decision || "решение";
      return `<div class="app-setting-card">
        <div class="app-setting-head">
          <strong class="mono">${esc(entry.tool || entry.grant_id || "контур")}</strong>
          <span class="badge">${esc(label)}</span>
        </div>
        <p class="muted small">${esc(when)}${entry.policy ? ` · ${esc(entry.policy)}` : ""}</p>
      </div>`;
    })
    .join("");
}

async function saveAppPermissions(inherit) {
  if (!ctx.appId) return;
  const write = inherit ? "inherit" : $("app-perm-write").value;
  const exec = inherit ? "inherit" : $("app-perm-exec").value;
  const permissions = { ...((ctx.config && ctx.config.permissions) || {}) };
  permissions.apps = { ...(permissions.apps || {}) };
  // Only explicit choices are stored; a missing field keeps following the
  // global profile, which is what "Как в профиле" promises.
  const override = {};
  if (write && write !== "inherit") override.write = write;
  if (exec && exec !== "inherit") override.exec = exec;
  if (Object.keys(override).length) {
    permissions.apps[ctx.appId] = override;
  } else {
    delete permissions.apps[ctx.appId];
  }
  ctx.config.permissions = permissions;
  await invoke("save_config", { config: ctx.config });
  ctx.chat = null;
  ctx.chatStart = null;
  ctx.chatId = null;
  ctx.chatItems = [];
  ctx.pendingPermissions = [];
  renderChat([]);
  renderPermissionPrompt();
  await ensureChat().catch(() => {});
  await refreshPermissionGrants();
  toast("Правила подтверждений сохранены");
}

async function loadConfig() {
  ctx.config = await invoke("get_config");
  renderLlmSettings();
  $("compact-auto").checked = !!ctx.config.compaction_auto;
  $("compact-threshold").value = ctx.config.compaction_threshold_percent || 80;
  $("q-session").value = ctx.config.session_quota_mb;
  $("q-global").value = ctx.config.global_quota_mb;
  $("q-ttl").value = ctx.config.ttl_hours;
  const defaults = (ctx.config.permissions && ctx.config.permissions.defaults) || {};
  $("perm-write").value = defaults.write || "ask";
  $("perm-exec").value = defaults.exec || "ask";
  await refreshPermissionGrants();
  renderReasoningOptions();
}

async function saveConfig() {
  await guard(async () => {
    const providers = collectLlmProviders();
    ctx.config = {
      ...(ctx.config || {}),
      llm_providers: providers,
      llm_default: $("llm-default-model").value || null,
      llm_models: (ctx.config && ctx.config.llm_models) || {},
      compaction_auto: $("compact-auto").checked,
      compaction_threshold_percent: Number($("compact-threshold").value) || 80,
      session_quota_mb: Number($("q-session").value) || 500,
      global_quota_mb: Number($("q-global").value) || 5120,
      ttl_hours: Number($("q-ttl").value) || 168,
      permissions: {
        ...((ctx.config && ctx.config.permissions) || {}),
        defaults: {
          write: $("perm-write").value,
          exec: $("perm-exec").value,
        },
      },
    };
    await invoke("save_config", { config: ctx.config });
    // Reload the normalized config: invalid providers/selections are dropped
    // server-side before the file is written.
    ctx.config = await invoke("get_config");
    ctx.chat = null;
    ctx.chatStart = null;
    ctx.chatItems = [];
    setBusy(false);
    renderChat([]);
    renderChatMeta();
    renderLlmSettings();
    renderReasoningOptions();
    ctx.pendingPermissions = [];
    renderPermissionPrompt();
    await refreshPermissionGrants();
    toast("Настройки сохранены, движок перезапустится при следующем сообщении");
  });
}

function renderLlmSettings() {
  ctx.llmDraft = cloneProviders(providersFromConfig(ctx.config));
  renderLlmProviderCards();
  renderLlmDefaultOptions();
  refreshLlmKeyStatuses();
}

function renderLlmProviderCards() {
  const box = $("llm-providers");
  if (!box) return;
  box.innerHTML = "";
  const providers = ctx.llmDraft || [];
  if (!providers.length) {
    box.innerHTML =
      '<p class="muted small">Провайдеры не настроены: движок запустится с демо-моделью.</p>';
    return;
  }
  providers.forEach((provider, index) => {
    const card = document.createElement("div");
    card.className = "llm-provider";
    card.dataset.providerIndex = String(index);
    card.innerHTML = `
      <div class="llm-provider-head">
        <label>Название<input data-llm="name" value="${esc(provider.name)}" placeholder="Корпоративный шлюз" /></label>
        <label>Id<input data-llm="id" class="mono" value="${esc(provider.id)}" placeholder="gw" /></label>
        <button class="ghost small danger-hover" data-llm-remove-provider type="button">Удалить</button>
      </div>
      <div class="grid">
        <label>Endpoint (OpenAI-compatible)<input data-llm="endpoint" value="${esc(provider.endpoint)}" placeholder="https://llm-gateway.internal/v1" /></label>
        <label>Ключ (DPAPI)<input data-llm="key" type="password" placeholder="ключ не задан" /></label>
        <div class="row-actions form-actions">
          <button class="ghost small" data-llm-key-save type="button">Сохранить ключ</button>
          <button class="ghost small danger-hover" data-llm-key-delete type="button">Удалить ключ</button>
          <span class="badge mono" data-llm-key-status>проверка…</span>
          <button class="ghost small" data-llm-probe type="button">Проверить TCP</button>
          <span class="muted small mono" data-llm-probe-out></span>
        </div>
      </div>
      <label class="check-row"><input data-llm="insecure" type="checkbox" ${provider.allow_invalid_certs ? "checked" : ""}/><span>Не проверять TLS-сертификат (закрытый контур, private CA)</span></label>
      <div class="llm-models"></div>
      <button class="ghost small" data-llm-add-model type="button">Добавить модель</button>`;
    const models = card.querySelector(".llm-models");
    provider.models.forEach((model, modelIndex) => {
      models.appendChild(renderLlmModelRow(model, modelIndex));
    });
    box.appendChild(card);
  });
}

function renderLlmModelRow(model, index) {
  const row = document.createElement("div");
  row.className = "llm-model";
  row.dataset.modelIndex = String(index);
  const caps = model.capabilities || capabilitiesOrDefault({});
  const price = pricingOrDefault(model.pricing);
  row.innerHTML = `
    <input data-llm-model="id" class="mono" value="${esc(model.id)}" placeholder="model id" />
    <input data-llm-model="name" value="${esc(model.name)}" placeholder="отображаемое имя" />
    <label class="check-row"><input data-llm-model="vision" type="checkbox" ${caps.vision ? "checked" : ""}/><span>vision</span></label>
    <label class="check-row"><input data-llm-model="reasoning" type="checkbox" ${caps.reasoning !== false ? "checked" : ""}/><span>reasoning</span></label>
    <input data-llm-model="efforts" value="${esc((caps.reasoning_efforts || []).join(","))}" placeholder="low,medium,high" />
    <input data-llm-model="context" type="number" min="0" value="${caps.context_tokens || ""}" placeholder="контекст" />
    <input data-llm-model="output" type="number" min="0" value="${caps.output_tokens || ""}" placeholder="вывод" />
    <input data-llm-model="price_in" type="number" min="0" step="0.001" value="${price.input_per_mtok ?? ""}" placeholder="вход/1M" title="Цена входа за 1M токенов" />
    <input data-llm-model="price_out" type="number" min="0" step="0.001" value="${price.output_per_mtok ?? ""}" placeholder="выход/1M" title="Цена выхода за 1M токенов" />
    <input data-llm-model="price_cr" type="number" min="0" step="0.001" value="${price.cache_read_per_mtok ?? ""}" placeholder="кэш чт/1M" title="Цена чтения кэша за 1M токенов" />
    <input data-llm-model="price_cw" type="number" min="0" step="0.001" value="${price.cache_write_per_mtok ?? ""}" placeholder="кэш зап/1M" title="Цена записи кэша за 1M токенов" />
    <input data-llm-model="price_cur" value="${esc(price.currency || "")}" placeholder="валюта" title="Валюта цен (например USD или RUB)" />
    <button class="ghost small danger-hover" data-llm-remove-model type="button" title="Удалить модель" aria-label="Удалить модель">×</button>`;
  return row;
}

function syncLlmDraft() {
  const providers = [];
  document.querySelectorAll("#llm-providers .llm-provider").forEach((card) => {
    const field = (name) => card.querySelector(`[data-llm="${name}"]`);
    const provider = {
      id: field("id").value.trim(),
      name: field("name").value.trim(),
      endpoint: field("endpoint").value.trim(),
      allow_invalid_certs: field("insecure").checked,
      models: [],
    };
    card.querySelectorAll(".llm-model").forEach((row) => {
      const read = (name) => row.querySelector(`[data-llm-model="${name}"]`);
      provider.models.push({
        id: read("id").value.trim(),
        name: read("name").value.trim(),
        capabilities: {
          vision: read("vision").checked,
          reasoning: read("reasoning").checked,
          reasoning_efforts: read("efforts")
            .value.split(",")
            .map((effort) => effort.trim().toLowerCase())
            .filter(Boolean),
          context_tokens: Number(read("context").value) || null,
          output_tokens: Number(read("output").value) || null,
        },
        pricing: {
          input_per_mtok: read("price_in").value === "" ? null : Number(read("price_in").value),
          output_per_mtok: read("price_out").value === "" ? null : Number(read("price_out").value),
          cache_read_per_mtok: read("price_cr").value === "" ? null : Number(read("price_cr").value),
          cache_write_per_mtok: read("price_cw").value === "" ? null : Number(read("price_cw").value),
          currency: read("price_cur").value.trim() || null,
        },
      });
    });
    providers.push(provider);
  });
  ctx.llmDraft = providers;
  return providers;
}

function collectLlmProviders() {
  const providers = syncLlmDraft()
    .map((provider) => ({
      id: provider.id,
      name: provider.name || null,
      endpoint: provider.endpoint,
      allow_invalid_certs: !!provider.allow_invalid_certs,
      models: provider.models
        .filter((model) => model.id)
        .map((model) => ({
          id: model.id,
          name: model.name || null,
          capabilities: {
            vision: !!model.capabilities.vision,
            reasoning: model.capabilities.reasoning !== false,
            reasoning_efforts: model.capabilities.reasoning_efforts.length
              ? [...new Set(model.capabilities.reasoning_efforts)]
              : ["low", "medium", "high"],
            context_tokens: model.capabilities.context_tokens || null,
            output_tokens: model.capabilities.output_tokens || null,
          },
          pricing: {
            input_per_mtok: model.pricing.input_per_mtok ?? null,
            output_per_mtok: model.pricing.output_per_mtok ?? null,
            cache_read_per_mtok: model.pricing.cache_read_per_mtok ?? null,
            cache_write_per_mtok: model.pricing.cache_write_per_mtok ?? null,
            currency: model.pricing.currency || null,
          },
        })),
    }))
    .filter((provider) => provider.id || provider.endpoint || provider.models.length);

  const seen = new Set();
  for (const provider of providers) {
    if (!/^[a-z0-9_-]{1,32}$/.test(provider.id)) {
      throw new Error(
        `id провайдера «${provider.id}»: только строчные латинские буквы, цифры, - и _`,
      );
    }
    if (seen.has(provider.id)) {
      throw new Error(`провайдер «${provider.id}» указан дважды`);
    }
    seen.add(provider.id);
    if (!provider.endpoint) throw new Error(`у провайдера «${provider.id}» нет endpoint`);
    if (!provider.models.length) throw new Error(`у провайдера «${provider.id}» нет моделей`);
  }
  return providers;
}

function renderLlmDefaultOptions() {
  const select = $("llm-default-model");
  if (!select) return;
  const previous = select.value;
  select.innerHTML = "";
  let count = 0;
  for (const provider of ctx.llmDraft || []) {
    for (const model of provider.models) {
      if (!provider.id || !model.id) continue;
      const option = document.createElement("option");
      option.value = `${provider.id}/${model.id}`;
      option.textContent = `${provider.name || provider.id} · ${model.name || model.id}`;
      select.appendChild(option);
      count += 1;
    }
  }
  if (!count) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "нет моделей";
    select.appendChild(option);
    select.disabled = true;
    return;
  }
  select.disabled = false;
  const wanted =
    (ctx.config && ctx.config.llm_default) || previous || (select.options[0] && select.options[0].value);
  const exists = [...select.options].some((option) => option.value === wanted);
  select.value = exists ? wanted : select.options[0].value;
}

async function refreshLlmKeyStatuses() {
  let names = [];
  try {
    names = await invoke("secrets_list");
  } catch (_) {
    names = [];
  }
  const has = (id) => names.includes(`llm.${id}`) || (id === "gw" && names.includes("llm_api_key"));
  document.querySelectorAll("#llm-providers .llm-provider").forEach((card) => {
    const id = card.querySelector('[data-llm="id"]').value.trim();
    const status = card.querySelector("[data-llm-key-status]");
    const input = card.querySelector('[data-llm="key"]');
    const present = has(id);
    if (status) {
      status.textContent = present ? "ключ в DPAPI" : "ключ не задан";
      status.className = "badge mono " + (present ? "ok" : "bad");
    }
    if (input) {
      input.placeholder = present ? "•••••••• (сохранён в DPAPI)" : "ключ не задан";
    }
  });
}

async function saveProviderKey(card) {
  const id = card.querySelector('[data-llm="id"]').value.trim();
  const input = card.querySelector('[data-llm="key"]');
  if (!/^[a-z0-9_-]{1,32}$/.test(id)) {
    return toast("Сначала укажите корректный id провайдера", true);
  }
  if (!input.value) return toast("Введите ключ", true);
  await invoke("secret_store", { name: `llm.${id}`, value: input.value });
  input.value = "";
  toast(`Ключ llm.${id} сохранён в DPAPI`);
  await refreshLlmKeyStatuses();
  await refreshSecrets();
}

async function deleteProviderKey(card) {
  const id = card.querySelector('[data-llm="id"]').value.trim();
  if (!/^[a-z0-9_-]{1,32}$/.test(id)) return;
  await invoke("secret_delete", { name: `llm.${id}` });
  toast(`Ключ llm.${id} удалён`);
  await refreshLlmKeyStatuses();
  await refreshSecrets();
}

async function probeProvider(card) {
  const url = card.querySelector('[data-llm="endpoint"]').value.trim();
  if (!url) return toast("Укажите endpoint", true);
  const out = card.querySelector("[data-llm-probe-out]");
  const result = await invoke("probe_endpoint", { url });
  if (out) {
    out.textContent = `${result.ok ? "✓ OK" : "✕ FAIL"} · ${result.millis} мс · ${result.detail}`;
  }
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
    await refreshLlmKeyStatuses();
  });
}

/* ---------- wiring ---------- */

function bind() {
  document.querySelectorAll(".tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      if (tab.dataset.tab === "settings") {
        ctx.settingsReturn =
          ctx.appId && !$("workspace").hidden ? "chat" : "catalog";
      }
      activateTab(tab.dataset.tab);
      if (tab.dataset.tab === "settings" && Date.now() - artifactsLastRefresh > 30000) {
        guard(refreshArtifacts);
      }
      if (tab.dataset.tab === "dev") guard(refreshPackTool);
      if (tab.dataset.tab === "apps") {
        showCatalog();
      } else if (tab.dataset.tab === "dev") {
        setViewTitle("Мастер пакета");
      } else if (tab.dataset.tab === "settings") {
        setViewTitle("Контур и секреты");
      }
    });
  });

  document.querySelectorAll("[data-mode]").forEach((btn) => {
    btn.addEventListener("click", () => chooseMode(btn.dataset.mode));
  });
  const modeChange = $("mode-change");
  if (modeChange) modeChange.addEventListener("click", showModeOverlay);
  const roleChange = $("role-change");
  if (roleChange) roleChange.addEventListener("click", changeRole);

  $("install-btn").addEventListener("click", installApp);
  $("install-pick-file").addEventListener("click", () => pickPackage("file"));
  $("install-pick-folder").addEventListener("click", () => pickPackage("folder"));

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
  if (wsBack) {
    wsBack.addEventListener("click", () => {
      const active = document.querySelector(".ws-tab.active");
      if (active && active.dataset.ws !== "chat") {
        showWorkspaceTab("chat");
        return;
      }
      showCatalog();
    });
  }
  $("ws-close").addEventListener("click", closeApp);

  const chatSearch = $("chat-search");
  if (chatSearch) {
    chatSearch.addEventListener("input", () => {
      ctx.chatQuery = chatSearch.value;
      renderChatList();
    });
  }

  const collapseBtn = $("btn-collapse");
  if (collapseBtn) {
    collapseBtn.addEventListener("click", () => setSidebarCollapsed(!sidebarCollapsed()));
  }
  const railHist = $("btn-rail-hist");
  if (railHist) railHist.addEventListener("click", openHistoryFromRail);

  $("confirm-ok").addEventListener("click", () => settleConfirm(true));
  $("confirm-cancel").addEventListener("click", () => settleConfirm(false));
  $("confirm-overlay").addEventListener("mousedown", (event) => {
    if (event.target === $("confirm-overlay")) settleConfirm(false);
  });

  const scrim = $("sheet-scrim");
  if (scrim) {
    scrim.addEventListener("mousedown", () => {
      closeSettings();
    });
  }
  const settingsClose = $("settings-close");
  if (settingsClose) {
    settingsClose.addEventListener("click", () => {
      closeSettings();
    });
  }

  document.addEventListener("mousedown", (event) => {
    const app = document.querySelector(".app");
    if (!app || !app.classList.contains("peek")) return;
    if (event.target.closest(".side")) return;
    app.classList.remove("peek");
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      const modelPop = $("chat-model-pop");
      if (modelPop && !modelPop.hidden) {
        closeModelMenu();
        return;
      }
      const reasoningPop = $("chat-reasoning-pop");
      if (reasoningPop && !reasoningPop.hidden) {
        closeReasoningMenu();
        return;
      }
      if (!$("confirm-overlay").hidden) {
        settleConfirm(false);
        return;
      }
      const app = document.querySelector(".app");
      if (app && app.classList.contains("peek")) {
        app.classList.remove("peek");
        return;
      }
      const scrim = $("sheet-scrim");
      if (scrim && !scrim.hidden) {
        closeSettings();
      }
      return;
    }
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    const key = String(event.key || "").toLowerCase();
    if (key === "b") {
      event.preventDefault();
      setSidebarCollapsed(!sidebarCollapsed());
    } else if (key === "k") {
      event.preventDefault();
      openHistoryFromRail();
    } else if (key === "n") {
      event.preventDefault();
      newChat();
    }
  });

  $("app-switch").addEventListener("change", (e) => {
    if (e.target.value && e.target.value !== ctx.appId) openApp(e.target.value);
  });
  $("ws-folder-pick").addEventListener("click", pickWorkspace);
  $("ws-folder-open").addEventListener("click", openWorkspaceFolder);

  $("settings-save").addEventListener("click", saveConfig);
  $("pack-pick").addEventListener("click", pickPackFolder);
  $("pack-load").addEventListener("click", loadPack);
  $("pack-import").addEventListener("click", importPack);
  $("pack-apply-template").addEventListener("click", applyPackTemplate);
  $("pack-init").addEventListener("click", initPack);
  $("pack-validate").addEventListener("click", () => runPackAction("validate"));
  $("pack-build").addEventListener("click", () => runPackAction("build"));
  $("pack-sign").addEventListener("click", () => runPackAction("sign"));
  $("pack-keygen").addEventListener("click", keygenPack);
  $("pack-install").addEventListener("click", installPack);
  $("pack-mcp-add").addEventListener("click", addPackMcp);
  $("pack-mcp").addEventListener("change", (event) => {
    const checkbox = event.target.closest("[data-mcp-write]");
    if (!checkbox) return;
    const dir = packDirValue();
    if (!dir) return;
    guard(async () => {
      const result = await invoke("pack_set_mcp", {
        dir,
        id: checkbox.dataset.mcpWrite,
        write: checkbox.checked,
      });
      await loadPack();
      packLog(result.output, result.ok);
    });
  });
  $("pack-save").addEventListener("click", savePackFile);
  $("pack-new").addEventListener("click", createPackFile);
  $("pack-files").addEventListener("click", (event) => {
    const row = event.target.closest("[data-pack-file]");
    if (row) selectPackFile(row.dataset.packFile);
  });
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
  const llmBox = $("llm-providers");
  if (llmBox) {
    llmBox.addEventListener("click", (event) => {
      const card = event.target.closest(".llm-provider");
      if (!card) return;
      const index = Number(card.dataset.providerIndex);
      if (event.target.closest("[data-llm-remove-provider]")) {
        syncLlmDraft();
        ctx.llmDraft.splice(index, 1);
        renderLlmProviderCards();
        renderLlmDefaultOptions();
        refreshLlmKeyStatuses();
        return;
      }
      if (event.target.closest("[data-llm-add-model]")) {
        syncLlmDraft();
        ctx.llmDraft[index].models.push({
          id: "",
          name: "",
          capabilities: capabilitiesOrDefault({}),
          pricing: pricingOrDefault({}),
        });
        renderLlmProviderCards();
        renderLlmDefaultOptions();
        refreshLlmKeyStatuses();
        return;
      }
      if (event.target.closest("[data-llm-remove-model]")) {
        syncLlmDraft();
        const row = event.target.closest(".llm-model");
        ctx.llmDraft[index].models.splice(Number(row.dataset.modelIndex), 1);
        renderLlmProviderCards();
        renderLlmDefaultOptions();
        refreshLlmKeyStatuses();
        return;
      }
      if (event.target.closest("[data-llm-key-save]")) {
        guard(() => saveProviderKey(card));
        return;
      }
      if (event.target.closest("[data-llm-key-delete]")) {
        guard(() => deleteProviderKey(card));
        return;
      }
      if (event.target.closest("[data-llm-probe]")) {
        guard(() => probeProvider(card));
      }
    });
    llmBox.addEventListener("input", (event) => {
      if (!event.target.matches('[data-llm="id"], [data-llm-model="id"]')) return;
      syncLlmDraft();
      renderLlmDefaultOptions();
    });
  }
  $("llm-provider-add").addEventListener("click", () => {
    syncLlmDraft();
    ctx.llmDraft.push({
      id: `provider${ctx.llmDraft.length + 1}`,
      name: "",
      endpoint: "",
      allow_invalid_certs: false,
      models: [{ id: "", name: "", capabilities: capabilitiesOrDefault({}), pricing: pricingOrDefault({}) }],
    });
    renderLlmProviderCards();
    renderLlmDefaultOptions();
    refreshLlmKeyStatuses();
  });
  $("chat-model-btn").addEventListener("click", (event) => {
    event.stopPropagation();
    const pop = $("chat-model-pop");
    if (pop && pop.hidden) openModelMenu();
    else closeModelMenu();
  });
  $("chat-model-pop").addEventListener("click", (event) => {
    const item = event.target.closest("[data-model]");
    if (item) {
      event.stopPropagation();
      guard(() => selectModel(item.dataset.model));
      return;
    }
    if (event.target.closest("#chat-model-settings")) {
      event.stopPropagation();
      closeModelMenu();
      const tab = document.querySelector('.tab[data-tab="settings"]');
      if (tab) tab.click();
    }
  });
  $("chat-reasoning-btn").addEventListener("click", (event) => {
    event.stopPropagation();
    const pop = $("chat-reasoning-pop");
    if (pop && pop.hidden) openReasoningMenu();
    else closeReasoningMenu();
  });
  $("chat-reasoning-pop").addEventListener("click", (event) => {
    const item = event.target.closest("[data-effort]");
    if (!item) return;
    event.stopPropagation();
    chooseReasoning(item.dataset.effort);
  });
  document.addEventListener("click", (event) => {
    if (
      event.target.closest("#chat-model-pop") ||
      event.target.closest("#chat-model-btn") ||
      event.target.closest("#chat-reasoning-pop") ||
      event.target.closest("#chat-reasoning-btn")
    ) {
      return;
    }
    closeModelMenu();
    closeReasoningMenu();
  });
  $("secret-save").addEventListener("click", saveSecret);
  $("secret-refresh").addEventListener("click", refreshSecrets);
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
  $("chat-edit-cancel").addEventListener("click", cancelEdit);
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
  $("chat-log").addEventListener("auxclick", (e) => {
    // A middle click must not navigate the webview to the link target.
    if (e.target.closest("[data-ext-link]")) e.preventDefault();
  });
  $("chat-log").addEventListener("click", (e) => {
    if (e.target.closest("[data-retry-turn]")) {
      guard(retryLastTurn);
      return;
    }
    const editBtn = e.target.closest("[data-edit-message]");
    if (editBtn) {
      const item = (ctx.chatItems || [])[Number(editBtn.dataset.editMessage)];
      beginEdit(item);
      return;
    }
    const regenBtn = e.target.closest("[data-regenerate]");
    if (regenBtn) {
      const index = Number(regenBtn.dataset.regenerate);
      const target = previousUserMessage(ctx.chatItems || [], index);
      if (target) guard(() => regenerateFrom(target));
      else toast("Не найдено сообщение для перегенерации", true);
      return;
    }
    const starter = e.target.closest("[data-starter]");
    if (starter) {
      $("chat-text").value = starter.dataset.starter || "";
      autosizeComposer();
      $("chat-text").focus();
      return;
    }
    if (e.target.closest("[data-diagram-action]")) {
      handleDiagramAction(e);
      return;
    }
    const codeCopy = e.target.closest("[data-copy-code]");
    if (codeCopy) {
      const codeNode = codeCopy.closest(".md-code")?.querySelector("pre code");
      navigator.clipboard.writeText(codeNode ? codeNode.textContent : "").then(
        () => toast("Код скопирован"),
        () => toast("Не удалось скопировать", true),
      );
      return;
    }
    const wsFile = e.target.closest("[data-ws-file]");
    if (wsFile) {
      openWorkspaceFile(wsFile.dataset.wsFile || "", wsFile.dataset.wsAction || "open");
      return;
    }
    const extLink = e.target.closest("[data-ext-link]");
    if (extLink) {
      e.preventDefault();
      openExternal(extLink.dataset.extLink || "");
      return;
    }
    const copy = e.target.closest("[data-copy]");
    if (!copy) return;
    navigator.clipboard.writeText(copy.dataset.text || "").then(
      () => toast("Скопировано в буфер обмена"),
      () => toast("Не удалось скопировать", true)
    );
  });
  $("chat-text").addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendChat();
    }
  });
  $("chat-text").addEventListener("input", autosizeComposer);

  $("chat-context").addEventListener("click", toggleContextPopover);
  $("chat-compact").addEventListener("click", (event) => {
    event.stopPropagation();
    closeContextPopover();
    guard(compactContext);
  });
  $("chat-context").addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggleContextPopover();
    }
    if (event.key === "Escape") closeContextPopover();
  });
  document.addEventListener("click", (event) => {
    if (event.target.closest("#chat-context, #chat-context-popover")) return;
    closeContextPopover();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeContextPopover();
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

  document.addEventListener("paste", async (event) => {
    const target = event.target;
    const editable =
      target && (target.matches?.("input, textarea") || target.isContentEditable);
    if (editable && target.id !== "chat-text") return;
    const clipboard = event.clipboardData;
    if (!clipboard) return;
    const files = [...(clipboard.files || [])];
    if (!files.length) {
      for (const item of clipboard.items || []) {
        if (item.kind === "file") {
          const file = item.getAsFile();
          if (file) files.push(file);
        }
      }
    }
    if (!files.length) return;
    event.preventDefault();
    await addAttachments(files);
  });

  const dropZone = document.querySelector(".chat-layout");
  if (dropZone) {
    let dragDepth = 0;
    const hasFiles = (event) => [...((event.dataTransfer && event.dataTransfer.types) || [])].includes("Files");
    dropZone.addEventListener("dragenter", (event) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      dragDepth += 1;
      dropZone.classList.add("drop-active");
    });
    dropZone.addEventListener("dragover", (event) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
    });
    dropZone.addEventListener("dragleave", () => {
      dragDepth = Math.max(0, dragDepth - 1);
      if (!dragDepth) dropZone.classList.remove("drop-active");
    });
    dropZone.addEventListener("drop", (event) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      dragDepth = 0;
      dropZone.classList.remove("drop-active");
      addAttachments(event.dataTransfer.files);
    });
  }

  document.querySelectorAll(".ws-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      showWorkspaceTab(tab.dataset.ws);
    });
  });

  $("secrets").addEventListener("click", (e) => {
    const del = e.target.closest("[data-secret]");
    if (del) deleteSecret(del.dataset.secret);
  });

  $("app-settings-refresh").addEventListener("click", () => guard(refreshAppSettings));
  $("app-memory-toggle").addEventListener("click", () =>
    guard(async () => {
      if (!ctx.memory || !ctx.appId) return;
      const enabled = !ctx.memory.enabled;
      const result = await invoke("memory_set_enabled", {
        app_id: ctx.appId,
        enabled,
      });
      if (result.applied) {
        if (result.restarted) {
          // The engine was stopped to rebuild its tool list; the next turn
          // starts a fresh one.
          ctx.chat = null;
        }
        toast(
          enabled
            ? result.restarted
              ? "Память включена, движок перезапущен"
              : "Память включена"
            : "Память выключена",
        );
      } else {
        ctx.memory.pendingRestart = true;
        toast("Память сохранена; применится после текущего хода");
      }
      await refreshAppMemory();
    }),
  );
  $("app-memory-search").addEventListener("input", () => {
    if (!ctx.memory) return;
    ctx.memory.query = $("app-memory-search").value.trim();
    clearTimeout(ctx.memorySearchTimer);
    ctx.memorySearchTimer = setTimeout(
      () => guard(() => loadAppMemory(true)),
      250,
    );
  });
  $("app-memory-more").addEventListener("click", () => guard(() => loadAppMemory(false)));
  $("app-memory-clear").addEventListener("click", () =>
    guard(async () => {
      if (!ctx.memory || !ctx.appId) return;
      const ok = await askConfirm({
        title: "Очистить память?",
        message: `Удалить ${ctx.memory.count || 0} фактов для этой рабочей папки? Действие необратимо.`,
        confirmLabel: "Очистить",
        danger: true,
      });
      if (!ok) return;
      const removed = await invoke("memory_clear", { app_id: ctx.appId });
      toast(`Удалено фактов: ${removed}`);
      await refreshAppMemory();
    }),
  );
  $("app-perm-save").addEventListener("click", () => guard(() => saveAppPermissions(false)));
  $("app-perm-inherit").addEventListener("click", () => guard(() => saveAppPermissions(true)));
  document.addEventListener("click", (e) => {
    const revoke = e.target.closest("[data-grant-revoke]");
    if (!revoke) return;
    guard(async () => {
      await invoke("permission_revoke_grant", { id: revoke.dataset.grantRevoke });
      await refreshPermissionGrants();
      toast("Разрешение отозвано");
    });
  });
  $("app-mcp-list").addEventListener("click", (e) => {
    const save = e.target.closest("[data-app-mcp-save]");
    if (save) return saveAppMcp(save.dataset.appMcpSave);
    const probe = e.target.closest("[data-app-mcp-probe]");
    if (probe) return probeAppMcp(probe.dataset.appMcpProbe);
    const reset = e.target.closest("[data-app-mcp-reset]");
    if (reset) return resetAppMcp(reset.dataset.appMcpReset);
  });
  $("app-secrets-list").addEventListener("click", (e) => {
    const save = e.target.closest("[data-app-secret-save]");
    if (save) return saveAppSecret(save.dataset.appSecretSave);
    const reset = e.target.closest("[data-app-secret-reset]");
    if (reset) return resetAppSecret(reset.dataset.appSecretReset);
  });
}

let currentMode = {
  environment: "corporate",
  role: "user",
  allow_authoring: false,
  mode_chosen: false,
};

function modeLabel(info) {
  const env = info.environment === "local" ? "локальный" : "корпоративный";
  const role = info.role === "developer" ? "разработчик" : "пользователь";
  return `${env} · ${role}`;
}

function applyMode(info) {
  currentMode = { ...currentMode, ...info };
  const label = modeLabel(currentMode);
  const badge = $("mode-badge");
  if (badge) {
    badge.textContent = label;
    badge.className = "badge mono " + (currentMode.environment === "local" ? "warn" : "ok");
    badge.title = "Режим установки и роль";
  }
  const setting = $("mode-setting");
  if (setting) {
    setting.textContent = label;
    setting.className = "badge " + (currentMode.environment === "local" ? "warn" : "ok");
  }
  const change = $("mode-change");
  if (change) {
    change.hidden = Boolean(currentMode.mode_locked);
    change.title = currentMode.mode_locked
      ? "Режим задан при установке; изменить его можно только переустановкой"
      : "Сменить режим установки";
  }
  const roleSetting = $("role-setting");
  if (roleSetting) {
    roleSetting.textContent = currentMode.role === "developer" ? "разработчик" : "пользователь";
    roleSetting.className = "badge " + (currentMode.role === "developer" ? "warn" : "");
  }
  const roleChange = $("role-change");
  if (roleChange) {
    roleChange.textContent = currentMode.role === "developer"
      ? "Переключить на пользователя"
      : "Включить режим разработчика";
  }
  const devTab = document.querySelector('.tab[data-tab="dev"]');
  if (devTab) devTab.style.display = currentMode.allow_authoring ? "" : "none";
  if (!currentMode.allow_authoring && devTab && devTab.classList.contains("active")) {
    const appsTab = document.querySelector('.tab[data-tab="apps"]');
    if (appsTab) appsTab.click();
  }
  if (currentMode.environment === "local") {
    const trust = $("trust");
    if (trust) {
      trust.textContent = "локальный слепок целостности";
      trust.className = "badge ok";
      trust.title = "Подпись не требуется: изменения промта, скилов и скриптов ловит локальный seal";
    }
    const allow = $("allowlist");
    if (allow) {
      allow.textContent = "allowlist: не требуется";
      allow.className = "badge";
      allow.title = "В локальном режиме ограничения allowlist не применяются";
    }
  }
}

function showModeOverlay() {
  const overlay = $("mode-overlay");
  if (!overlay) return;
  const error = $("mode-error");
  if (error) error.hidden = true;
  const role = document.querySelector(
    `input[name="mode-role"][value="${currentMode.role}"]`
  );
  if (role) role.checked = true;
  overlay.hidden = false;
}

function hideModeOverlay() {
  const overlay = $("mode-overlay");
  if (overlay) overlay.hidden = true;
}

async function chooseMode(environment) {
  const role =
    document.querySelector('input[name="mode-role"]:checked')?.value || "user";
  const error = $("mode-error");
  if (error) error.hidden = true;
  try {
    await invoke("set_mode", { environment, role });
    hideModeOverlay();
    await refreshPlatformInfo();
    await refreshApps();
    toast(`Режим: ${modeLabel(currentMode)}`);
  } catch (e) {
    if (error) {
      error.textContent = String(e);
      error.hidden = false;
    } else {
      toast(String(e), true);
    }
  }
}

async function changeRole() {
  const role = currentMode.role === "developer" ? "user" : "developer";
  try {
    await invoke("set_role", { role });
    await refreshPlatformInfo();
    await refreshApps();
    toast(`Роль: ${modeLabel(currentMode)}`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function refreshPlatformInfo() {
  const info = await invoke("platform_info");
  renderPlatformInfo(info);
  return info;
}

async function init() {
  initTheme();
  bind();
  bindChatEvents();
  bindPermissionUi();
  try {
    setSidebarCollapsed(localStorage.getItem(SIDEBAR_KEY) === "1");
  } catch (_) {}
  await guard(async () => {
    const info = await invoke("platform_info");
    renderPlatformInfo(info);
    if (!info.mode_chosen) showModeOverlay();
    await loadConfig();
    await refreshLlmKeyStatuses();
    await refreshApps();
    await refreshArtifacts();
    await refreshSecrets();
    await refreshMcpServers();
  });
}

function renderPlatformInfo(info) {
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
    applyMode(info);
}

window.addEventListener("DOMContentLoaded", init);
