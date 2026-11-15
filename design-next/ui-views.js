/* Каталог пакетов, MCP-консоль, мастер пакета. */
(function (A) {
  "use strict";
  const { $, $$, esc, IC } = A;

  /* ── каталог ───────────────────────────────────────────── */
  A.renderCatalog = function () {
    $("#apps-count").textContent = A.AGENTS.length;
    $("#cards").innerHTML = A.AGENTS.map((ag) => {
      const blocked = !ag.trusted;
      const trust = A.trustState(ag);
      const openAttr = blocked ? 'data-blocked="1"' : 'data-open="' + ag.id + '"';
      return '<article class="card ' + (blocked ? "blocked" : "") + '" ' + openAttr + ' tabindex="0" role="button" aria-label="' + esc(ag.name) + '">' +
        '<div class="c-head"><span class="av ' + (ag.av || "") + '">' + esc(ag.initials) + "</span>" +
        '<span class="c-name"><span class="cw">' + esc(ag.name) + '</span><small>v' + esc(ag.ver) + " · " + esc(ag.id) + "</small></span>" +
        '<span class="badge ' + trust.cls + '">' + esc(trust.badge) + "</span></div>" +
        '<div class="c-pub">' + esc(ag.publisher) + "</div>" +
        (blocked ? '<div class="c-problem">' + esc(ag.problem) + "</div>" : "") +
        '<div class="c-tools">' + ag.tools.map((t) =>
          '<div class="c-tool"><span class="g ' + t.kind + '"></span><span class="n">' + esc(t.name) + "</span>" +
          '<span class="tag ' + (t.kind === "write" ? "w" : "r") + '">' + esc(t.kind) + "</span></div>").join("") + "</div>" +
        '<div class="c-foot"><span class="path">' + esc(ag.folder) + "</span>" +
        '<span class="go">' + (blocked ? "заблокирован" : "Открыть " + IC.arrow) + "</span></div></article>";
    }).join("");
  };

  /* ── MCP-консоль ───────────────────────────────────────── */
  A.MCP_TOOLS = {
    context7: ["resolve-library-id", "query-docs"],
    opensearch: ["query-opensearch", "index-stats"],
  };

  A.audit = A.audit || [
    { t: "14:41:22", name: "docx_export", app: "doc-writer", st: "allowed" },
    { t: "14:41:14", name: "write_file", app: "doc-writer", st: "allowed" },
    { t: "14:40:02", name: "push-ticket", app: "incident-brief", st: "blocked" },
    { t: "14:38:55", name: "ses-301", app: "doc-writer", st: "opened" },
  ];

  A.renderAudit = function () {
    const el = $("#tool-audit");
    if (!el) return;
    const statuses = { allowed: "разрешён", blocked: "отказ", opened: "открыт" };
    $("#audit-count").textContent = A.audit.length;
    el.innerHTML = A.audit.slice(0, 7).map((r) =>
      '<div class="artifact"><span class="mono" style="color:var(--dim)">' + esc(r.t) + "</span>" +
      '<span class="nm">' + esc(r.name) + '</span><span class="sz">' + esc(r.app) + "</span>" +
      '<span class="tag ' + (r.st === "blocked" ? "x" : r.st === "allowed" ? "r" : "") + '" style="margin-left:8px">' + esc(statuses[r.st] || r.st) + "</span></div>").join("");
  };

  A.allowedServers = function (ag) {
    return (A.config.mcp || []).filter((m) => (ag.connections || []).includes(m.id));
  };

  A.allowedMcpTools = function (ag, server) {
    const manifest = new Set((ag.tools || []).map((t) => t.name));
    return (A.MCP_TOOLS[server] || []).filter((t) => manifest.has(t));
  };

  A.sampleArgs = function (tool) {
    const samples = {
      "resolve-library-id": '{\n  "libraryName": "Tauri",\n  "query": "command permissions"\n}',
      "query-docs": '{\n  "libraryId": "/tauri-apps/tauri",\n  "query": "capability scope"\n}',
      "query-opensearch": '{\n  "index": "logs-*",\n  "query": "level:error",\n  "minutes": 60\n}',
      "index-stats": '{\n  "index": "logs-*"\n}',
    };
    return samples[tool] || '{\n  "query": ""\n}';
  };

  A.renderTools = function () {
    const ag = A.agent();
    const trust = A.trustState(ag);
    const write = A.writePolicy();
    $("#manifest-tools").innerHTML =
      ag.tools.map((t) =>
        '<div class="mtool"><span class="dot ' + (t.kind === "read" ? "" : "warn") + '"></span>' +
        '<span class="nm">' + esc(t.name) + "</span>" +
        '<span class="tag ' + (t.kind === "write" ? "w" : t.kind === "exec" ? "" : "r") + '">' + esc(t.kind) + "</span></div>").join("") +
      '<div class="mtool deny"><span class="dot idle"></span><span class="nm">shell</span><span class="tag x">deny</span></div>' +
      '<div class="mtool deny"><span class="dot idle"></span><span class="nm">http_fetch</span><span class="tag x">deny</span></div>';

    $("#tools-profile").innerHTML =
      '<div class="kv"><span>Файлы</span><b>workspace</b></div>' +
      '<div class="kv"><span>Внешняя запись</span><b class="' + write.cls + '">' + write.label + "</b></div>" +
      '<div class="kv"><span>Лимит ответа</span><b>64 КиБ</b></div>' +
      '<div class="kv"><span>Пакет</span><b class="' + trust.cls + '">' + esc(trust.label) + "</b></div>" +
      '<div class="kv"><span>Модель</span><b>' + esc(A.S.model.split("/").slice(-1)[0]) + "</b></div>";

    const servers = A.allowedServers(ag);
    const previous = $("#server-select").value;
    $("#server-select").innerHTML = servers.length
      ? servers.map((m) => '<option value="' + esc(m.id) + '">' + esc(m.id) + "</option>").join("")
      : '<option value="">нет MCP-подключений</option>';
    if (previous && servers.some((m) => m.id === previous)) $("#server-select").value = previous;
    A.fillToolSelect();
    A.renderAudit();
    if (!$("#tool-args").value) {
      $("#tool-args").value = A.sampleArgs($("#tool-select").value);
    }
  };

  A.fillToolSelect = function () {
    const server = $("#server-select").value;
    const list = A.allowedMcpTools(A.agent(), server);
    const previous = $("#tool-select").value;
    $("#tool-select").innerHTML = list.length
      ? list.map((t) => "<option>" + esc(t) + "</option>").join("")
      : '<option value="">нет разрешённых</option>';
    if (previous && list.includes(previous)) $("#tool-select").value = previous;
    const selected = $("#tool-select").value || "";
    const argsEl = $("#tool-args");
    if (argsEl && argsEl.dataset.tool !== selected) {
      argsEl.value = selected ? A.sampleArgs(selected) : "";
      argsEl.dataset.tool = selected;
    }
    const call = $("#tool-call");
    if (call) call.disabled = !list.length;
    const sample = $("#tool-sample");
    if (sample) sample.disabled = !list.length;
    if (!list.length) {
      $("#tool-meta").textContent = server ? "нет разрешённых MCP-инструментов" : "нет MCP-подключений";
      $("#tool-meta").className = "badge warn";
      $("#tool-result").hidden = true;
      $("#tool-empty").hidden = false;
      $("#tool-empty").textContent = server
        ? "У пакета нет MCP-инструментов для этого сервера в allow-list манифеста."
        : "У текущего пакета нет MCP-подключений. Откройте агента с MCP в каталоге.";
    } else if ($("#tool-meta").className.indexOf("warn") !== -1) {
      $("#tool-meta").textContent = "готов к вызову";
      $("#tool-meta").className = "badge";
      $("#tool-empty").textContent = "Выполните вызов инструмента, чтобы увидеть ответ MCP-сервера и размер полезной нагрузки.";
    }
  };

  A.runToolCall = function () {
    const server = $("#server-select").value;
    const tool = $("#tool-select").value;
    if (!server || !A.allowedMcpTools(A.agent(), server).includes(tool)) {
      $("#tool-meta").textContent = "вызов запрещён allow-list";
      $("#tool-meta").className = "badge bad";
      A.toast("Инструмент не входит в allow-list пакета для этого сервера", "err");
      return;
    }
    const args = $("#tool-args").value.trim() || "{}";
    let parsed;
    try { parsed = JSON.parse(args); } catch (e) {
      $("#tool-meta").textContent = "ошибка JSON";
      $("#tool-meta").className = "badge bad";
      A.toast("Аргументы не являются валидным JSON", "err");
      return;
    }
    $("#tool-meta").textContent = "вызов…";
    $("#tool-meta").className = "badge";
    $("#tool-empty").hidden = true;
    $("#tool-result").hidden = false;
    $("#tool-result").textContent = "// ожидание ответа от " + server + "…";
    setTimeout(() => {
      const payload = {
        server, tool, args: parsed, ok: true, truncated: false,
        content: tool === "resolve-library-id"
          ? [{ libraryName: "Tauri", id: "/tauri-apps/tauri", trust: "high", snippets: 214 }]
          : tool === "query-docs"
            ? [{ title: "Capabilities", note: "windows, permissions, scope", bytes: 18432 }]
            : [{ note: "демонстрационный ответ MCP-сервера" }],
        audit: { actor: A.config.role, app: A.agent().id, ms: 42 },
      };
      const text = JSON.stringify(payload, null, 2);
      $("#tool-result").textContent = text;
      $("#tool-meta").textContent = "ответ " + (text.length / 1024).toFixed(1) + " КиБ · 42 мс";
      $("#tool-meta").className = "badge ok";
      A.audit.unshift({ t: A.now(), name: tool, app: A.agent().id, st: "allowed" });
      A.renderAudit();
      A.toast("Вызов " + tool + " выполнен · записан в аудит");
    }, 650);
  };

  /* ── мастер пакета ─────────────────────────────────────── */
  A.renderWizard = function () {
    const W = A.W;
    const note = $("#wiz-role-note");
    if (note) note.hidden = A.config.role === "developer";
    $("#pack-files").innerHTML = Object.keys(W.files).map((name) =>
      '<button class="pack-file ' + (name === W.current ? "on" : "") + '" data-file="' + esc(name) + '">' + IC.file +
      "<span>" + esc(name) + '</span><span class="sz">' + A.fmtSize(W.files[name].content.length) + "</span></button>").join("");
    $("#pack-mcp").innerHTML = W.mcp.map((m) =>
      '<div class="list-row"><span class="mono">' + esc(m.id) + "</span>" +
      '<span class="tag ' + (m.write ? "w" : "r") + '">' + (m.write ? "rw" : "read") + "</span>" +
      '<button class="del" data-act="del-mcp" data-id="' + esc(m.id) + '" title="Удалить">' + IC.cross + "</button></div>").join("") ||
      '<div class="inline-note" style="margin:0">подключений нет</div>';
    $("#pack-dir").value = W.dir;
    $("#pack-app-id").value = W.appId;
    $("#pack-name").value = W.name;
    $("#pack-template").value = W.template;
    A.loadFile(W.current);
    A.renderLog();
  };

  A.loadFile = function (name) {
    if (!A.W.files[name]) return;
    A.W.current = name;
    $("#pack-editor").value = A.W.files[name].content;
    $("#pack-editor-path").textContent = name;
    $$("#pack-files .pack-file").forEach((b) => b.classList.toggle("on", b.dataset.file === name));
  };

  A.wlog = function (text, cls) {
    A.W.log.push({ t: new Date().toLocaleTimeString("ru-RU", { hour12: false }), text, cls: cls || "" });
    A.renderLog();
  };

  A.renderLog = function () {
    const el = $("#pack-log");
    if (!A.W.log.length) { el.hidden = true; return; }
    el.hidden = false;
    el.innerHTML = A.W.log.map((l) => '<span class="t">' + l.t + '</span>  <span class="' + l.cls + '">' + esc(l.text) + "</span>").join("\n");
    el.scrollTop = el.scrollHeight;
  };
})(window.App);
