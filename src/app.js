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

    document.querySelectorAll(".ws-tab").forEach((t) => t.classList.toggle("active", t.dataset.ws === "chat"));
    $("ws-chat").hidden = false;
    $("ws-tools").hidden = true;
    $("ws-settings").hidden = true;
    $("chat-log").innerHTML = "";
    renderChatList();
    renderChat([]);
    renderChatMeta();
    ensureChat().catch((e) => chatError(String(e)));

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
    toast("Открываю рабочую папку");
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
  return text
    .split(/(`[^`]+`)/)
    .map((part) => {
      if (part.length > 2 && part.startsWith("`") && part.endsWith("`")) {
        const inner = part.slice(1, -1);
        const name = workspaceFileRef(inner);
        return name ? fileChipHtml(name) : `<code>${inner}</code>`;
      }
      return linkifyBareFilesOutsideLinks(linkifyUrls(part));
    })
    .join("")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>");
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
  ].join("\u0001");
}

function buildChatNode(item, streaming) {
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
    const node = buildChatNode(items[i], streaming);
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
  if (atBottom) log.scrollTop = log.scrollHeight;
  if (ctx.busy) renderThinking();
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
}

function renderThinking() {
  const log = $("chat-log");
  if (!log) return;
  let node = log.querySelector(".chat-thinking");
  if (!ctx.busy) {
    if (node) node.remove();
    return;
  }
  if (!node) {
    node = document.createElement("div");
    node.className = "chat-thinking";
    node.innerHTML = '<span class="spinner" aria-hidden="true"></span><span class="chat-thinking-text"></span>';
  }
  const secs = Math.max(0, Math.round((Date.now() - busyStart) / 1000));
  node.querySelector(".chat-thinking-text").textContent = secs ? `думает… ${secs} с` : "думает…";
  if (log.lastElementChild !== node) log.appendChild(node);
  log.scrollTop = log.scrollHeight;
}

function renderChatMeta(model) {
  const endpoint = (ctx.config && ctx.config.llm_endpoint) || "";
  const modelText = model || (ctx.chat && ctx.chat.model) || "движок не запущен";
  $("chat-model").textContent = modelText;
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

function renderChatList() {
  const box = $("chat-list");
  box.innerHTML = "";
  const query = (ctx.chatQuery || "").trim().toLowerCase();
  const chats = (ctx.chats || []).filter(
    (chat) => !query || String(chat.title || "").toLowerCase().includes(query),
  );
  let group = "";
  for (const chat of chats) {
    const label = chatGroup(chat.updated);
    if (label !== group) {
      group = label;
      const head = document.createElement("div");
      head.className = "hist-group";
      head.textContent = label;
      box.appendChild(head);
    }
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

    const del = document.createElement("button");
    del.className = "chat-del";
    del.textContent = "×";
    del.title = "Удалить чат";
    del.dataset.del = chat.id;

    row.appendChild(btn);
    row.appendChild(del);
    box.appendChild(row);
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
  renderPermissionPrompt();
  return ctx.chat;
}

async function newChat() {
  if (!ctx.appId) {
    toast("Сначала выберите приложение в каталоге", true);
    return;
  }
  await guard(async () => {
    showWorkspace();
    await ensureChat().catch(() => {});
    const info = await invoke("chat_new", { app_id: ctx.appId });
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
  if (!ctx.appId) return;
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
  try {
    await ensureChat();
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
      refreshChats();
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
    ctx.streamingItems = [];
    ctx.pendingPermissions = (ctx.pendingPermissions || []).filter(
      (item) => item.session_id !== ctx.chatId,
    );
    renderPermissionPrompt();
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
    let info = null;
    try {
      info = await invoke("mcp_server_get", { id });
    } catch (_) {}
    const row = document.createElement("div");
    row.className = "secret-row";
    const tls = info && info.allow_invalid_certs ? '<span class="badge bad">TLS off</span>' : "";
    row.innerHTML = `<span>${esc(id)} ${tls}</span><div class="row-actions">
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
    await invoke("mcp_server_save", {
      id,
      url,
      token: $("mcp-token").value || null,
      allow_invalid_certs: $("mcp-insecure").checked,
    });
    $("mcp-id").value = "";
    $("mcp-url").value = "";
    $("mcp-token").value = "";
    $("mcp-insecure").checked = false;
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
      '<span class="muted small">MCP не объявлены; endpoint и токен задаются в Настройках</span>';
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
  $("llm-endpoint").value = ctx.config.llm_endpoint || "";
  $("llm-model").value = ctx.config.llm_model || "";
  $("llm-insecure").checked = !!ctx.config.llm_allow_invalid_certs;
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
  const defaults = (ctx.config.permissions && ctx.config.permissions.defaults) || {};
  $("perm-write").value = defaults.write || "ask";
  $("perm-exec").value = defaults.exec || "ask";
  await refreshPermissionGrants();
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
      llm_allow_invalid_certs: $("llm-insecure").checked,
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
      permissions: {
        ...((ctx.config && ctx.config.permissions) || {}),
        defaults: {
          write: $("perm-write").value,
          exec: $("perm-exec").value,
        },
      },
    };
    await invoke("save_config", { config: ctx.config });
    ctx.chat = null;
    ctx.chatItems = [];
    setBusy(false);
    renderChat([]);
    renderChatMeta();
    renderReasoningOptions();
    ctx.pendingPermissions = [];
    renderPermissionPrompt();
    await refreshPermissionGrants();
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
  if (wsBack) wsBack.addEventListener("click", showCatalog);
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
      activateTab("apps");
      showCatalog();
    });
  }
  const settingsClose = $("settings-close");
  if (settingsClose) {
    settingsClose.addEventListener("click", () => {
      activateTab("apps");
      showCatalog();
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
        activateTab("apps");
        showCatalog();
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
  $("chat-log").addEventListener("auxclick", (e) => {
    // A middle click must not navigate the webview to the link target.
    if (e.target.closest("[data-ext-link]")) e.preventDefault();
  });
  $("chat-log").addEventListener("click", (e) => {
    const starter = e.target.closest("[data-starter]");
    if (starter) {
      $("chat-text").value = starter.dataset.starter || "";
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
      document.querySelectorAll(".ws-tab").forEach((t) => t.classList.toggle("active", t === tab));
      for (const name of ["chat", "tools", "settings"]) {
        const body = $(`ws-${name}`);
        if (body) body.hidden = name !== tab.dataset.ws;
      }
      if (tab.dataset.ws === "settings") guard(refreshAppSettings);
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

  $("app-settings-refresh").addEventListener("click", () => guard(refreshAppSettings));
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
    await refreshLlmKeyStatus();
    await refreshApps();
    await refreshArtifacts();
    await refreshPackTool(true);
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
