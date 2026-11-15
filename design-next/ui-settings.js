/* Шторка агента, настройки платформы, поповеры агента и папки. */
(function (A) {
  "use strict";
  const { $, $$, esc, IC } = A;

  A.renderAgentSheet = function () {
    const ag = A.agent();
    const trust = A.trustState(ag);
    const write = A.writePolicy();
    const allow = A.config.mode === "local"
      ? { label: "не используется", cls: "" }
      : ag.allow ? { label: "разрешён", cls: "ok" } : { label: "нет записи", cls: "bad" };
    $("#sh-av").textContent = ag.initials;
    $("#sh-av").className = "av lg " + (ag.av || "");
    $("#sh-name").textContent = ag.name;
    $("#sh-sub").textContent = ag.id + " · v" + ag.ver + " · " + ag.publisher;
    $$("#sheet-agent-tabs .sheet-tab").forEach((t) => t.classList.toggle("on", t.dataset.atab === A.S.dialogTab));

    const body = $("#sheet-agent-body");
    if (A.S.dialogTab === "access") {
      body.innerHTML =
        '<div class="grp"><div class="grp-h">Целостность</div>' +
        '<div class="kv"><span>Подпись</span><b class="' + trust.cls + '">' +
        (ag.trusted ? (trust.cls === "warn" ? "локальный слепок" : "проверена · prod") : "не проверена") + "</b></div>" +
        '<div class="kv"><span>Seal</span><b>' + esc(ag.sealHash || "—") + "</b></div>" +
        '<div class="kv"><span>Allowlist</span><b class="' + allow.cls + '">' + allow.label + "</b></div></div>" +
        '<div class="grp"><div class="grp-h">Доступ</div>' +
        '<div class="kv"><span>Рабочая папка</span><b>' + esc(A.S.folder || ag.folder) + "</b></div>" +
        '<div class="kv"><span>Файлы вне папки</span><b class="bad">DENY</b></div>' +
        '<div class="kv"><span>Внешняя запись</span><b class="' + write.cls + '">' + write.label + "</b></div>" +
        '<div class="kv"><span>Сеть</span><b>' + Math.max(0, ag.connections.length - 1) + " MCP</b></div></div>" +
        '<div class="grp"><div class="grp-h">Инструменты · allow-list манифеста</div>' +
        ag.tools.map((t) => '<div class="tool-row"><span class="mono">' + esc(t.name) + "</span>" +
          '<span class="tag ' + (t.kind === "write" ? "w" : t.kind === "exec" ? "" : "r") + '">' +
          esc(t.kind) + (t.approve ? " · спрашивать" : "") + "</span></div>").join("") +
        '<div class="tool-row" style="opacity:.6"><span class="mono">shell</span><span class="tag x">deny</span></div>' +
        '<div class="tool-row" style="opacity:.6"><span class="mono">http_fetch</span><span class="tag x">deny</span></div></div>' +
        '<div class="grp"><div class="grp-h">Подключения</div>' +
        ag.connections.map((c) => '<div class="kv"><span>' + esc(c) + "</span><b>" +
          (c === "llm" ? "глобальный шлюз" : "override") + "</b></div>").join("") + "</div>" +
        '<div class="grp"><div class="grp-h">Квота сессии</div>' +
        '<div class="kv"><span>Использовано</span><b>42 МБ из ' + A.config.q_session + " МБ</b></div>" +
        '<div class="kv"><span>TTL дампов</span><b>' + A.config.q_ttl + " ч</b></div></div>";
    } else if (A.S.dialogTab === "mcp") {
      const mcp = ag.connections.filter((c) => c !== "llm");
      body.innerHTML =
        '<p class="inline-note" style="margin:0 0 12px">Один и тот же id может иметь разные URL, токены и TLS-политику в разных приложениях. Без override используется глобальный реестр.</p>' +
        (mcp.map((c) => {
          const g = A.config.mcp.find((m) => m.id === c) || { id: c, url: "не задан", tls: true };
          return '<div class="grp"><div class="grp-h">' + esc(c) + '<span class="sp"></span><span class="badge warn">override</span></div>' +
            '<div class="kv"><span>URL приложения</span><b>' + esc(g.url) + "</b></div>" +
            '<div class="kv"><span>TLS</span><b class="' + (g.tls ? "ok" : "warn") + '">' + (g.tls ? "проверяется" : "private CA") + "</b></div>" +
            '<div class="form-actions"><button class="btn ghost sm" data-act="mcp-override" data-id="' + esc(c) + '">Изменить</button>' +
            '<button class="btn ghost sm" data-act="mcp-reset" data-id="' + esc(c) + '">Сбросить на глобальное</button></div></div>';
        }).join("") || '<div class="inline-note">У агента нет MCP-подключений.</div>') +
        '<div class="divider"></div><div class="sec-label">Добавить подключение</div>' +
        '<div class="form-actions" style="margin-top:0"><input class="inp" id="app-mcp-new" placeholder="id подключения" style="flex:1;min-width:120px" />' +
        '<button class="btn sm" data-act="mcp-add-app">Добавить</button></div>' +
        '<p class="inline-note">В манифест пакета попадает только id — endpoint и токен живут в DPAPI.</p>';
    } else {
      const list = A.appSecrets[ag.id] || [];
      body.innerHTML =
        '<p class="inline-note" style="margin:0 0 12px">Секреты, объявленные в <b class="mono">tools[].secrets</b>. Значения хранятся в DPAPI с namespace приложения и не попадают в пакет.</p>' +
        '<div class="grp"><div class="grp-h">Секреты приложения</div>' +
        (list.length ? list.map((s) => '<div class="list-row"><span class="mono">' + esc(s.name) + '</span><span class="meta">' + esc(s.note) +
          '</span><button class="del" data-act="del-appsecret" data-name="' + esc(s.name) + '" title="Удалить">' + IC.cross + "</button></div>").join("")
          : '<div class="inline-note">Секретов нет.</div>') + "</div>" +
        '<div class="sec-label">Добавить значение</div><div class="form-grid">' +
        '<label class="field">Имя<input id="app-secret-name" placeholder="docx_export.template" /></label>' +
        '<label class="field">Значение<input id="app-secret-value" type="password" placeholder="•••••••" /></label></div>' +
        '<div class="form-actions"><button class="btn sm" data-act="appsecret-add">Зашифровать и сохранить</button></div>';
    }
  };

  /* ── настройки ─────────────────────────────────────────── */
  A.fillSettings = function () {
    const c = A.config;
    $("#set-mode-badge").textContent = c.mode === "corporate" ? "корпоративный" : "локальный";
    $("#set-mode-badge").className = "badge " + (c.mode === "corporate" ? "ok" : "accent");
    $("#set-role").textContent = c.role === "developer" ? "разработчик" : "пользователь";
    $("#set-anchor").textContent = c.mode === "corporate" ? "CN=ECO Root 2026" : "не требуется";
    $("#set-anchor").className = c.mode === "corporate" ? "ok" : "";
    const allowed = A.AGENTS.filter((x) => x.trusted).length;
    $("#set-allowlist").textContent = c.mode === "corporate"
      ? allowed + " " + A.plural(allowed, ["пакет", "пакета", "пакетов"])
      : "не используется";
    $("#set-role-toggle").textContent = c.role === "developer" ? "Вернуть роль пользователя" : "Включить режим разработчика";

    $("#set-llm-endpoint").value = c.llm_endpoint;
    $("#set-llm-model").value = c.llm_model;
    $("#set-key-status").textContent = c.llm_key ? "DPAPI · задан" : "не задан";
    $("#set-key-status").className = "badge mono " + (c.llm_key ? "ok" : "warn");
    $("#set-llm-key").value = "";
    $("#set-llm-key").placeholder = c.llm_key ? "задан — введите новый, чтобы заменить" : "ключ не задан";
    $("#set-vision").checked = !!c.vision;
    $("#set-reasoning").checked = !!c.reasoning;
    $("#set-efforts").value = c.efforts;
    $("#set-context").value = c.context;
    $("#set-output").value = c.output;
    $("#set-insecure").checked = !!c.insecure;
    $("#set-q-session").value = c.q_session;
    $("#set-q-global").value = c.q_global;
    $("#set-q-ttl").value = c.q_ttl;

    $("#set-mcp-list").innerHTML = c.mcp.length
      ? c.mcp.map((m) => '<div class="list-row"><span class="mono">' + esc(m.id) + "</span>" +
        '<span class="tag ' + (m.tls ? "r" : "w") + '">' + (m.tls ? "TLS ок" : "private CA") + "</span>" +
        '<button class="del" data-act="del-mcp-global" data-id="' + esc(m.id) + '" title="Удалить">' + IC.cross + "</button></div>").join("")
      : '<div class="inline-note">Серверов нет.</div>';

    $("#set-secret-list").innerHTML = c.secrets.length
      ? c.secrets.map((s) => '<div class="list-row"><span class="mono">' + esc(s.name) + '</span><span class="meta">' + esc(s.note) +
        '</span><button class="del" data-act="del-secret" data-name="' + esc(s.name) + '" title="Удалить">' + IC.cross + "</button></div>").join("")
      : '<div class="inline-note">Секретов нет.</div>';

    $("#set-art-app").innerHTML = A.AGENTS.filter((x) => x.trusted).map((x) => '<option value="' + esc(x.id) + '">' + esc(x.name) + "</option>").join("");
    $("#set-art-app").value = A.S.agentId === "incident-brief" ? "doc-writer" : A.S.agentId;
    A.fillArtSessions();
    $("#set-art-usage").textContent = "42 МБ / " + Math.round(c.q_global / 1024) + " ГБ";
    A.renderArtList();
  };

  A.fillArtSessions = function () {
    const app = $("#set-art-app").value;
    const list = A.SESSIONS.filter((s) => s.agentId === app);
    $("#set-art-session").innerHTML = '<option value="">все сессии</option>' +
      list.map((s) => '<option value="' + esc(s.id) + '">' + esc(s.id) + " · " + esc(s.title) + "</option>").join("");
  };

  A.renderArtList = function (query) {
    const app = $("#set-art-app").value;
    const ses = $("#set-art-session").value;
    const q = (query !== undefined ? query : $("#set-art-query").value || "").trim().toLowerCase();
    const rows = [];
    A.SESSIONS.filter((s) => s.agentId === app && (!ses || s.id === ses)).forEach((s) => {
      (A.artifacts[s.id] || []).forEach((f) => {
        if (!q || f.name.toLowerCase().includes(q)) rows.push(Object.assign({ ses: s.id }, f));
      });
    });
    $("#set-art-list").innerHTML = rows.length
      ? rows.map((r) => '<div class="artifact"><span class="mono">' + esc(r.ses) + '</span><span class="nm">' + esc(r.name) +
        '</span><span class="sz">' + esc(r.size) + "</span></div>").join("")
      : '<div class="inline-note">Ничего не найдено.</div>';
  };

  A.readSettings = function () {
    const c = A.config;
    c.llm_endpoint = $("#set-llm-endpoint").value.trim() || c.llm_endpoint;
    c.llm_model = $("#set-llm-model").value.trim() || c.llm_model;
    c.vision = $("#set-vision").checked;
    c.reasoning = $("#set-reasoning").checked;
    c.efforts = $("#set-efforts").value.trim() || "low,medium,high";
    c.context = Number($("#set-context").value) || c.context;
    c.output = Number($("#set-output").value) || c.output;
    c.insecure = $("#set-insecure").checked;
    c.q_session = Number($("#set-q-session").value) || c.q_session;
    c.q_global = Number($("#set-q-global").value) || c.q_global;
    c.q_ttl = Number($("#set-q-ttl").value) || c.q_ttl;
    if ($("#set-llm-key").value.trim()) { c.llm_key = true; $("#set-llm-key").value = ""; }
  };

  A.applyConfig = function () {
    A.store.set("eco.config", A.config);
    A.S.model = A.config.llm_model;
    A.updateComposer();
    A.renderCatalog();
    A.renderSidebar();
    A.renderAgentSheet();
    A.fillSettings();
    A.renderTools();
    A.renderWizard();
  };

  A.toggleRole = function () {
    A.config.role = A.config.role === "developer" ? "user" : "developer";
    A.store.set("eco.config", A.config);
    A.applyConfig();
    A.toast("Роль: " + (A.config.role === "developer" ? "разработчик" : "пользователь"));
  };

  /* ── поповеры ──────────────────────────────────────────── */
  A.renderAgentPop = function () {
    $("#pop-agent").innerHTML =
      '<div class="h">Агенты</div>' +
      A.AGENTS.map((x) =>
        '<button class="it ' + (x.id === A.S.agentId ? "sel" : "") + '" data-pick-agent="' + esc(x.id) + '"' + (x.trusted ? "" : " disabled") + ">" +
        '<span class="av ' + (x.av || "") + '">' + esc(x.initials) + "</span>" +
        '<span class="t2">' + esc(x.name) + "<em>" + esc(x.id) + " · v" + esc(x.ver) + "</em></span>" +
        '<small style="color:' + (x.trusted ? "var(--ok)" : "var(--bad)") + '">' + (x.trusted ? "●" : "БЛОК") + "</small></button>").join("") +
      '<div class="sep"></div>' +
      '<button class="it" data-goto="catalog">' + IC.plus + "Все агенты и каталог</button>" +
      '<button class="it" data-goto="install"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3"/></svg>Установить пакет…<small>.zip</small></button>';
  };

  A.renderFolderPop = function () {
    const ag = A.agent();
    const folders = ag.folders || [ag.folder];
    const cur = A.S.folder || ag.folder;
    $("#pop-folder").innerHTML =
      '<div class="h">Рабочая папка</div>' +
      folders.map((f) => '<button class="it ' + (f === cur ? "sel" : "") + '" data-folder="' + esc(f) + '">' +
        '<span class="mono" style="font-size:11.5px">' + esc(f) + "</span></button>").join("") +
      '<div class="sep"></div><button class="it" data-folder-pick="1">Выбрать другую…</button>';
  };
})(window.App);
