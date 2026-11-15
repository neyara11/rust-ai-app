/* Диалог: отправка, симуляция ответа агента, подтверждения, установка пакетов, экспорт. */
(function (A) {
  "use strict";
  const { $, esc } = A;

  const clearTimers = () => { A.S.timers.forEach(clearTimeout); A.S.timers = []; };
  A.clearTimers = clearTimers;

  A.send = function () {
    const ta = $("#chat-text");
    const text = ta.value.trim();
    if (!text && !A.S.attachments.length) { ta.focus(); return; }
    if (A.S.busy) return;
    const ses = A.session();
    if (!ses) return;

    ses.items.push({
      t: "user", text: text || "Посмотри прикреплённые файлы.", at: A.now(),
      files: A.S.attachments.map((f) => ({ name: f.name, size: f.size })),
    });
    if (ses.title === "Новый диалог") {
      ses.title = (text || "Новый диалог").slice(0, 48) + (text.length > 48 ? "…" : "");
      A.renderTopbar();
    }
    A.S.attachments = [];
    ta.value = "";
    ta.style.height = "auto";
    A.S.busy = true;
    ses.items.push({ t: "typing", label: "агент думает…" });
    A.renderThread();
    A.renderSidebar();

    A.S.timers.push(setTimeout(() => {
      if (!A.S.busy) return;
      ses.items = ses.items.filter((i) => i.t !== "typing");
      A.simulateAgent(ses);
      A.S.busy = false;
      A.renderThread();
    }, 1150));
  };

  A.stopGeneration = function () {
    if (!A.S.busy) return;
    clearTimers();
    A.S.busy = false;
    const ses = A.session();
    if (ses) ses.items = ses.items.filter((i) => i.t !== "typing");
    A.renderThread();
    A.toast("Генерация остановлена", "warn");
  };

  A.simulateAgent = function (ses) {
    const ag = A.agent();
    const model = A.config.llm_model.split("/").slice(-1)[0];
    if (ag.id === "doc-writer") {
      ses.items.push({
        t: "steps",
        think: "Проверю папку и обновлю документы. Для Word понадобится docx_export с подтверждением.",
        calls: [
          { name: "platform_list_files", detail: "· 6 файлов", time: "0.07 с", ok: true },
          { name: "platform_write_file", detail: "· report.md", time: "0.05 с", ok: true },
        ],
        dur: "1.9 с",
      });
      ses.items.push({
        t: "agent",
        html: "<p>Обновил <code>report.md</code>: добавил раздел и пересобрал структуру плана.</p>" +
          '<table><tr><th>Файл</th><th>Изменение</th><th>Размер</th></tr>' +
          '<tr><td class="mono">report.md</td><td>+ раздел «Блокировки»</td><td class="mono">3.4 КБ</td></tr></table>',
      });
      ses.items.push({
        t: "ask", id: A.uid("ask"), state: "open",
        q: 'Перезаписать <b class="mono">report.docx</b>?',
        sub: "docx_export · запись в рабочую папку · " + (A.S.folder || ag.folder),
      });
    } else if (ag.id === "doc-lookup") {
      ses.items.push({
        t: "steps",
        think: "Уточню версию библиотеки и найду раздел про capability-файлы.",
        calls: [
          { name: "resolve-library-id", detail: "· /tauri-apps/tauri", time: "0.18 с", ok: true },
          { name: "query-docs", detail: "· 2 раздела, 12 КиБ", time: "0.51 с", ok: true },
        ],
        dur: "1.6 с",
      });
      ses.items.push({
        t: "agent",
        html: "<p>Доступ ограничивается capability-файлом: в нём перечислены окна и scope путей.</p>" +
          '<pre>{\n  <span class="s">"identifier"</span>: <span class="s">"reports-readonly"</span>,\n' +
          '  <span class="s">"windows"</span>: [<span class="s">"main"</span>],\n' +
          '  <span class="s">"permissions"</span>: [<span class="s">"fs:allow-read-text-file"</span>]\n}</pre>' +
          "<p>Запись не выдаётся вовсе — для одного окна и одной папки этого достаточно.</p>",
      });
    } else if (ag.id === "incident-triage") {
      ses.items.push({
        t: "steps",
        think: "Соберу события за окно инцидента и сгруппирую по сообщению.",
        calls: [
          { name: "query-opensearch", detail: "· 4 218 событий", time: "1.1 с", ok: true },
          { name: "chart_render", detail: "· топ-5 ошибок", time: "0.28 с", ok: true },
        ],
        dur: "2.4 с",
      });
      ses.items.push({
        t: "agent",
        html: "<p>После 04:12 доминирует <code>connection pool exhausted</code> — 61% событий. Ниже — распределение.</p>" +
          '<table><tr><th>Ошибка</th><th>Событий</th><th>Первый раз</th></tr>' +
          '<tr><td class="mono">pool exhausted</td><td class="mono">2 571</td><td class="mono">04:12:38</td></tr>' +
          '<tr><td class="mono">upstream 502</td><td class="mono">1 034</td><td class="mono">04:12:41</td></tr>' +
          '<tr><td class="mono">timeout 5s</td><td class="mono">613</td><td class="mono">04:13:02</td></tr></table>',
      });
    } else {
      ses.items.push({
        t: "steps",
        think: "Возьму метрики из выгрузки и посчитаю доступность по окнам.",
        calls: [
          { name: "query-opensearch", detail: "· 30 дней", time: "0.8 с", ok: true },
          { name: "chart_render", detail: "· график SLA", time: "0.3 с", ok: true },
        ],
        dur: "1.7 с",
      });
      ses.items.push({
        t: "agent",
        html: "<p>Доступность за период — <strong>99.62%</strong>, целевой уровень 99.5% выполнен. Просадка одна: 4 ноября, 46 минут.</p>" +
          A.fileRow("sla-report.md", "6.1 КБ"),
      });
    }
    markLastAgent(ses, "1 080 → 246 tok · 52 tok/s · " + model);
  };

  function markLastAgent(ses, meta) {
    for (let i = ses.items.length - 1; i >= 0; i--) {
      if (ses.items[i].t === "agent") { ses.items[i].meta = meta; return; }
    }
  }

  A.resolveAsk = function (id, allow) {
    const ses = A.session();
    if (!ses) return;
    const it = ses.items.find((i) => i.t === "ask" && i.id === id);
    if (!it || it.state !== "open") return;
    it.state = allow ? "allowed" : "denied";
    it.at = A.now();
    A.renderThread();
    A.toast(allow ? "Вызов разрешён и записан в аудит" : "Вызов отклонён — агент получил отказ", allow ? "" : "warn");
    if (!allow) return;

    A.S.busy = true;
    A.updateComposer();
    A.S.timers.push(setTimeout(() => {
      ses.items.push({
        t: "agent",
        meta: "420 → 96 tok · 61 tok/s · " + A.config.llm_model.split("/").slice(-1)[0],
        html: "<p><code>report.docx</code> перезаписан, стили шаблона сохранены.</p>" + A.fileRow("report.docx", "15.2 КБ"),
      });
      const art = A.artifacts[ses.id] || (A.artifacts[ses.id] = []);
      if (!art.some((f) => f.name === "report.docx")) art.push({ name: "report.docx", size: "15.2 КБ" });
      A.S.busy = false;
      A.renderThread();
    }, 900));
  };

  /* ── установка пакета ──────────────────────────────────── */
  A.installPackage = function (path, quick) {
    const status = $("#install-status");
    status.hidden = false;
    const name = (path || "").split(/[\\/]/).filter(Boolean).pop() || "";
    const known = A.AGENTS.find((x) => x.id === name);
    const local = A.config.mode === "local";

    if (known && known.id === "incident-brief") {
      if (local) {
        known.trusted = true; known.seal = true; known.allow = true;
        known.problem = null; known.publisher = "локальный слепок"; known.sealHash = "local:9ac41f";
        known.desc = "Экспорт разбора инцидента во внешний трекер. Установлен с локальным слепком целостности.";
        known.subs = ["Собери бриф по INC-2291", "Отправь разбор в трекер", "Что изменилось за сутки?"];
        status.className = "badge warn";
        status.textContent = "установлен · локальный seal";
        A.toast("Пакет без подписи установлен с локальным слепком целостности", "warn");
        A.renderCatalog();
        A.selectAgent("incident-brief");
      } else {
        status.className = "badge bad";
        status.textContent = "отклонён контуром";
        A.toast("Пакет отклонён: нет подписи доверенного ключа и записи в allowlist", "err");
      }
      return;
    }
    if (known) {
      status.className = "badge ok";
      status.textContent = "уже установлен";
      A.toast("Пакет уже установлен — открываю " + known.name);
      A.selectAgent(known.id);
      return;
    }
    if (!name) {
      status.hidden = true;
      A.toast("Укажите путь к .zip или папке пакета", "err");
      return;
    }
    if (!local && !quick) {
      status.className = "badge bad";
      status.textContent = "нет подписи";
      A.toast("В корпоративном контуре пакет должен быть подписан ключом из trust anchor", "err");
      return;
    }
    const id = name.toLowerCase().replace(/[^a-z0-9-]/g, "-");
    A.AGENTS.push({
      id, name, initials: name.slice(0, 2).toUpperCase(), av: "v", ver: "0.1.0",
      publisher: local ? "локальный слепок" : "CN=ECO AIOps Dev", trusted: true, seal: true, allow: true,
      sealHash: "local:3f7b20", folder: "D:\\reports\\" + id, folders: ["D:\\reports\\" + id],
      desc: "Установленный пакет. Контракт инструментов берётся из манифеста.",
      tools: [{ name: "platform_list_files", kind: "read" }, { name: "platform_write_file", kind: "write" }],
      connections: ["llm"], subs: ["Что лежит в папке?", "Сделай выжимку"],
    });
    status.className = "badge ok";
    status.textContent = "установлен";
    A.toast("Пакет установлен и проверен");
    A.renderCatalog();
    A.SESSIONS.unshift({ id: A.nextSessionId(), agentId: id, title: "Новый диалог", group: "Сегодня", items: [], folder: "D:\\reports\\" + id });
    A.selectAgent(id);
  };

  /* ── экспорт протокола ─────────────────────────────────── */
  A.exportThread = function () {
    const ses = A.session();
    if (!ses) return;
    const lines = ["# " + ses.title, "", "Сессия: " + ses.id + " · агент: " + A.agent().id, ""];
    ses.items.forEach((it) => {
      if (it.t === "user") lines.push("**Оператор:** " + it.text.replace(/<[^>]+>/g, ""), "");
      if (it.t === "agent") lines.push("**Агент:** " + it.html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim(), "");
      if (it.t === "steps") lines.push("_Вызовы:_ " + it.calls.map((c) => c.name).join(", "), "");
      if (it.t === "ask") lines.push("_Подтверждение:_ " + it.sub + " — " + it.state, "");
    });
    try {
      const blob = new Blob([lines.join("\n")], { type: "text/markdown" });
      const link = document.createElement("a");
      link.href = URL.createObjectURL(blob);
      link.download = "protocol-" + ses.id + ".md";
      link.click();
      setTimeout(() => URL.revokeObjectURL(link.href), 1000);
      A.toast("Протокол выгружен в Markdown");
    } catch (e) {
      A.toast("Не удалось выгрузить протокол", "err");
    }
  };
})(window.App);
