/* Данные прототипа: каталог агентов, конфигурация, мастер пакета. */
(function (A) {
  "use strict";

  A.AGENTS = [
    {
      id: "doc-writer", name: "Документы и Word", initials: "DW", av: "", ver: "0.1.0",
      publisher: "CN=ECO AIOps Dev", trusted: true, seal: true, allow: true, sealHash: "7f3a9c41d0b8e2f6",
      desc: "Пишет и правит документы в рабочей папке и собирает Word-версии через платформенный экспорт.",
      folder: "D:\\reports\\db-degradation",
      folders: ["D:\\reports\\db-degradation", "D:\\reports\\sla-2026", "D:\\docs\\akt"],
      tools: [
        { name: "platform_list_files", kind: "read" },
        { name: "platform_write_file", kind: "write" },
        { name: "docx_export", kind: "write", approve: true },
      ],
      connections: ["llm"],
      subs: ["Создай отчёт по инциденту", "Сконвертируй report.md в Word", "Собери акт приёмки", "Что изменилось в папке?"],
    },
    {
      id: "doc-lookup", name: "Справка по библиотекам", initials: "DL", av: "g", ver: "0.9.2",
      publisher: "CN=ECO AIOps Security", trusted: true, seal: true, allow: true, sealHash: "48efcf9d72861a68",
      desc: "Ищет актуальную документацию библиотек через MCP context7. Только чтение, без записи на диск.",
      folder: "D:\\docs\\tauri",
      folders: ["D:\\docs\\tauri", "D:\\docs\\python", "D:\\docs\\infra"],
      tools: [
        { name: "resolve-library-id", kind: "read" },
        { name: "query-docs", kind: "read" },
      ],
      connections: ["llm", "context7"],
      subs: ["Права в Tauri v2", "Найди API для File System", "Сравни capability и scope", "Документация по mcp"],
    },
    {
      id: "incident-triage", name: "Разбор инцидентов", initials: "IT", av: "v", ver: "0.4.0",
      publisher: "CN=ECO AIOps Dev", trusted: true, seal: true, allow: true, sealHash: "b91c77a0f4d2e613",
      desc: "Подтягивает логи из OpenSearch, группирует ошибки и готовит черновик разбора инцидента.",
      folder: "D:\\cases\\INC-2291",
      folders: ["D:\\cases\\INC-2291", "D:\\cases\\INC-2277"],
      tools: [
        { name: "query-opensearch", kind: "read" },
        { name: "chart_render", kind: "exec" },
      ],
      connections: ["llm", "opensearch"],
      subs: ["Разбери INC-2291", "Топ ошибок за сутки", "Построй график 5xx", "Найди первый сбой"],
    },
    {
      id: "sla-metrics", name: "SLA и метрики", initials: "SL", av: "v", ver: "0.2.3",
      publisher: "CN=ECO AIOps Dev", trusted: true, seal: true, allow: true, sealHash: "c2d4a0f19e7b553a",
      desc: "Считает SLA по выгрузкам и собирает отчёт с графиками. Запись — только в собственную папку.",
      folder: "D:\\reports\\sla-2026",
      folders: ["D:\\reports\\sla-2026", "D:\\reports\\sla-2025"],
      tools: [
        { name: "query-opensearch", kind: "read" },
        { name: "chart_render", kind: "exec" },
        { name: "platform_write_file", kind: "write" },
      ],
      connections: ["llm", "opensearch"],
      subs: ["SLA за октябрь", "Сравни с прошлым месяцем", "Собери отчёт с графиком", "Где просадка 4.11?"],
    },
    {
      id: "incident-brief", name: "Экспорт в трекер", initials: "IB", av: "r", ver: "0.3.1",
      publisher: "CN=Untrusted Vendor", trusted: false, seal: false, allow: false, sealHash: "",
      desc: "Экспорт разбора инцидента во внешний трекер. Пакет отклонён контуром.",
      problem: "hash mismatch · prompt/system.md",
      folder: "D:\\cases\\outbox",
      folders: ["D:\\cases\\outbox"],
      tools: [{ name: "push-ticket", kind: "write" }],
      connections: ["llm"],
      subs: [],
    },
  ];

  A.DEFAULT_CONFIG = {
    mode: "corporate",
    role: "user",
    llm_endpoint: "https://llm-gateway.internal/v1",
    llm_model: "~deepseek/deepseek-v4-flash-latest",
    llm_key: true,
    vision: false,
    reasoning: true,
    efforts: "low,medium,high",
    context: 128000,
    output: 32000,
    insecure: false,
    q_session: 500,
    q_global: 5120,
    q_ttl: 168,
    mcp: [
      { id: "context7", url: "https://mcp.context7.com/mcp", tls: true },
      { id: "opensearch", url: "https://os.internal:9200/mcp", tls: false },
    ],
    secrets: [
      { name: "llm_api_key", note: "DPAPI · задан" },
      { name: "opensearch_token", note: "DPAPI · задан" },
    ],
  };

  A.config = Object.assign({}, A.DEFAULT_CONFIG, A.store.get("eco.config", {}));

  A.appSecrets = {
    "doc-writer": [{ name: "docx_export.template", note: "DPAPI · задан" }],
    "doc-lookup": [],
    "incident-triage": [{ name: "opensearch_token", note: "override · DPAPI" }],
    "sla-metrics": [],
    "incident-brief": [],
  };

  A.artifacts = {
    "ses-301": [
      { name: "report.md", size: "2.1 КБ" },
      { name: "report.docx", size: "14.8 КБ" },
      { name: "ses-301-toolcalls.ndjson", size: "1.4 МБ", dump: true },
    ],
    "ses-201": [{ name: "ses-201-toolcalls.ndjson", size: "820 КБ", dump: true }],
    "ses-401": [
      { name: "inc-2291-5xx.png", size: "148 КБ" },
      { name: "ses-401-toolcalls.ndjson", size: "2.6 МБ", dump: true },
    ],
  };

  let sessionSeq = 302;
  A.nextSessionId = () => "ses-" + sessionSeq++;

  A.agent = () => A.AGENTS.find((x) => x.id === A.S.agentId) || A.AGENTS[0];  A.session = () => A.SESSIONS.find((x) => x.id === A.S.sessionId) || null;
  A.agentSessions = () => A.SESSIONS.filter((x) => x.agentId === A.S.agentId);

  /* ── выбор агента и сессии ─────────────────────────────── */
  A.selectSession = function (id) {
    const s = A.SESSIONS.find((x) => x.id === id);
    if (!s) return;
    A.S.sessionId = id;
    A.S.agentId = s.agentId;
    const ag = A.agent();
    A.S.folder = s.folder || ag.folder;
    A.S.effort = s.effort || "medium";
    A.S.attachments = [];
    A.S.busy = false;
    A.setView("chat");
  };

  A.selectAgent = function (id) {
    A.closePops();
    const ag = A.AGENTS.find((x) => x.id === id);
    if (!ag) return;
    if (!ag.trusted) {
      A.toast("Пакет заблокирован: " + (ag.problem || "нарушена целостность"), "err");
      return;
    }
    A.S.agentId = id;
    A.S.folder = ag.folder;
    const last = A.SESSIONS.filter((s) => s.agentId === id)[0];
    if (last) {
      A.S.sessionId = last.id;
      A.S.effort = last.effort || "medium";
    } else {
      const s = { id: A.nextSessionId(), agentId: id, title: "Новый диалог", group: "Сегодня", items: [], folder: ag.folder };
      A.SESSIONS.unshift(s);
      A.S.sessionId = s.id;
    }
    A.S.attachments = [];
    A.setView("chat");
  };

  A.newSession = function () {
    const s = {
      id: A.nextSessionId(), agentId: A.S.agentId, title: "Новый диалог",
      group: "Сегодня", items: [], folder: A.S.folder,
    };
    A.SESSIONS.unshift(s);
    A.S.sessionId = s.id;
    A.S.attachments = [];
    A.S.busy = false;
    A.S.search = "";
    A.$("#search-input").value = "";
    A.setView("chat");
    A.$("#chat-text").focus();
  };

  /* ── мастер пакета: исходное состояние ─────────────────── */
  A.W = {
    dir: "D:\\apps\\doc-writer",
    appId: "doc-writer",
    name: "Документы и Word",
    template: "files",
    current: "manifest.json",
    mcp: [{ id: "context7", write: false }],
    log: [],
    signed: false,
    files: {
      "manifest.json": {
        content: '{\n  "schema": 2,\n  "app_id": "doc-writer",\n  "name": "Документы и Word",\n  "version": "0.1.0",\n  "entry": "AGENTS.md",\n  "prompt": "prompt/system.md",\n  "capabilities": { "file_scopes": ["workspace"] },\n  "tools": [\n    { "tool": "platform_list_files", "stream": "read" },\n    { "tool": "platform_write_file", "stream": "write" },\n    { "tool": "docx_export", "stream": "write", "approve": true }\n  ],\n  "connections": [\n    { "id": "llm", "type": "llm", "required": true },\n    { "id": "context7", "type": "http-mcp", "required": false }\n  ]\n}',
      },
      "prompt/system.md": {
        content: "# Системный промпт\n\nТы — агент «Документы и Word». Работаешь только внутри рабочей папки,\nвыбранной оператором. Не выходи за её пределы, не обращайся в сеть\nнапрямую: единственный внешний канал — объявленные MCP-инструменты.\n\n## Правила\n1. Перед записью файла кратко опиши, что собираешься сделать.\n2. Любая перезапись существующего файла требует подтверждения.\n3. Если данных не хватает — спроси, не выдумывай.\n4. Отвечай по-русски, структурированно, с указанием имён файлов.",
      },
      "skills/docx/SKILL.md": {
        content: "---\nname: docx-export\ndescription: Конвертация markdown в Word через platform_docx_export\n---\n\n# Когда применять\nОператор просит .docx, «вордовскую версию», «документ для рассылки».\n\n# Как\n1. Убедись, что исходный .md лежит в рабочей папке.\n2. Вызови docx_export с путём к файлу и шаблоном по умолчанию.\n3. Верни ссылки на оба файла и размеры.",
      },
      "AGENTS.md": {
        content: "# Документы и Word\n\nМини-приложение для работы с документами в изолированной папке.\n\n## Инструменты\n- platform_list_files — чтение списка файлов\n- platform_write_file — запись в рабочую папку\n- docx_export — конвертация markdown → docx (требует подтверждения)\n",
      },
    },
  };
})(window.App);
