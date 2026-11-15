/* Демо-лента диалогов: сессии, сообщения, вызовы инструментов, подтверждения. */
(function (A) {
  "use strict";

  const fileRow = (name, size) =>
    '<div class="file-row">' + A.IC.file + "<span>" + A.esc(name) + '</span><span class="sz">' + A.esc(size) +
    '</span><button data-act="artifact" data-file="' + A.esc(name) + '">открыть</button></div>';

  const u = (text, at, files) => ({ t: "user", text, at: at || "16:00", files: files || [] });
  const a = (html, meta) => ({ t: "agent", html, meta });
  const steps = (think, calls, dur, open) => ({ t: "steps", think, calls, dur, open: !!open });
  const ask = (id, q, sub) => ({ t: "ask", id, q, sub, state: "open" });
  const call = (name, detail, time, ok) => ({ name, detail, time, ok: ok !== false });
  const foot = (meta) => ({ t: "agentfoot", meta });

  A.fileRow = fileRow;

  A.SESSIONS = [
    {
      id: "ses-301", agentId: "doc-writer", title: "План проверки деградации БД", group: "Сегодня",
      items: [
        u("В рабочей папке создай <b>report.md</b> с планом проверки гипотезы о деградации БД и сконвертируй его в Word.", "15:52"),
        steps("Посмотрю файлы в рабочей папке, затем запишу отчёт и вызову docx_export.", [
          call("platform_list_files", "· 4 файла", "0.08 с"),
          call("platform_write_file", "· report.md, 2.1 КБ", "0.04 с"),
          call("docx_export", "· report.docx", "1.12 с"),
        ], "3.4 с", true),
        a("<h3>Готово</h3><p>Создал в рабочей папке два файла. Markdown можно править и пересобирать — Word-версия перезапишется.</p>" +
          "<table><tr><th>Файл</th><th>Что внутри</th><th>Размер</th></tr>" +
          '<tr><td class="mono">report.md</td><td>план проверки: метрики, гипотезы, критерии</td><td class="mono">2.1 КБ</td></tr>' +
          '<tr><td class="mono">report.docx</td><td>Word-версия через <code>docx_export</code></td><td class="mono">14.8 КБ</td></tr></table>' +
          "<p><strong>Структура плана:</strong></p><ul>" +
          "<li>симптомы — рост <code>p95</code> запросов к <code>orders</code> с 40 до 310 мс;</li>" +
          "<li>три гипотезы: блокировки, раздувание индексов, nightly-vacuum;</li>" +
          "<li>для каждой — запрос к <code>pg_stat_statements</code> и критерий отказа.</li></ul>",
          "980 → 210 tok · 58 tok/s · deepseek-v4-flash"),
        u("Добавь раздел с SQL для первой гипотезы и пересобери docx.", "15:55"),
        a('<p>Добавлю раздел «Блокировки» с запросом к <code>pg_locks</code>:</p>' +
          '<pre><span class="k">SELECT</span> l.pid, l.mode, a.query, now() - a.query_start <span class="k">AS</span> age\n' +
          '<span class="k">FROM</span>   pg_locks l <span class="k">JOIN</span> pg_stat_activity a <span class="k">USING</span> (pid)\n' +
          '<span class="k">WHERE</span>  <span class="k">NOT</span> l.granted\n' +
          '<span class="k">ORDER BY</span> age <span class="k">DESC</span>;  <span class="c">-- ожидающие блокировки</span></pre>'),
        ask("ask-301", 'Перезаписать <b class="mono">report.docx</b>?', "docx_export · запись в рабочую папку · D:\\reports\\db-degradation"),
      ],
    },
    {
      id: "ses-298", agentId: "doc-writer", title: "report.md → Word", group: "Сегодня",
      items: [
        u("Сконвертируй report.md в Word и положи рядом.", "14:10"),
        a("<p>Готово — <code>report.docx</code> рядом с исходником, стили шаблона сохранены.</p>" + fileRow("report.docx", "14.8 КБ")),
      ],
    },
    {
      id: "ses-287", agentId: "doc-writer", title: "Шаблон акта приёмки", group: "Вчера",
      items: [
        u("Собери акт приёмки по шаблону из прошлой недели и подставь дату 14.11.", "11:24"),
        steps("Найду последний акт в папке и скопирую структуру.", [call("platform_list_files", "· 1 шаблон", "0.06 с")], "1.1 с"),
        a("<p>Собрал <code>akt-2026-11-14.md</code> по шаблону. Отличия от прошлого акта — только дата и номер.</p>" + fileRow("akt-2026-11-14.md", "3.4 КБ")),
      ],
    },
    {
      id: "ses-281", agentId: "doc-writer", title: "Сводка INC-2291 в docx", group: "Неделя",
      items: [
        u("Переложи сводку по INC-2291 в docx для рассылки.", "Пн"),
        a("<p>Переложил, добавил титул и колонтитул. Таблицы не разъехались.</p>" + fileRow("inc-2291-summary.docx", "96 КБ")),
      ],
    },
    {
      id: "ses-201", agentId: "doc-lookup", title: "Разграничение прав в Tauri v2", group: "Сегодня",
      items: [
        u("Как в Tauri v2 ограничить доступ плагина к файлам только одной папкой?", "13:02"),
        steps("Найду раздел про permissions и capabilities в документации Tauri v2.", [
          call("resolve-library-id", "· Tauri → /tauri-apps/tauri", "0.21 с"),
          call("query-docs", "· 3 раздела, 18 КиБ", "0.64 с"),
        ], "2.1 с"),
        a("<h3>Коротко</h3><p>В Tauri v2 доступ задаётся не в коде, а capability-файлами: плагин объявляет permission, приложение выдаёт его только нужному окну и только для нужного scope.</p>" +
          "<table><tr><th>Слой</th><th>Что задаёт</th></tr>" +
          '<tr><td class="mono">permission</td><td>что вообще разрешено плагину</td></tr>' +
          '<tr><td class="mono">capability</td><td>какому окну и с каким scope</td></tr>' +
          '<tr><td class="mono">scope</td><td>конкретные пути: <code>$APPDATA/reports/**</code></td></tr></table>' +
          "<p>Для «только одна папка» выдайте <code>fs:allow-write-text-file</code> со scope на эту папку и не включайте <code>fs:default</code>.</p>",
          "1 240 → 380 tok · 44 tok/s · deepseek-v4-flash"),
      ],
    },
    {
      id: "ses-198", agentId: "doc-lookup", title: "Permissions: capability files", group: "Вчера",
      items: [
        u("Покажи пример capability-файла с двумя окнами.", "16:40"),
        a('<p>Вот минимальный пример: главному окну — запись в отчёты, окну настроек — только чтение.</p>' +
          '<pre>{\n  <span class="s">"identifier"</span>: <span class="s">"main-window"</span>,\n  <span class="s">"windows"</span>: [<span class="s">"main"</span>],\n  <span class="s">"permissions"</span>: [\n    { <span class="s">"identifier"</span>: <span class="s">"fs:allow-write-text-file"</span>,\n      <span class="s">"allow"</span>: [{ <span class="s">"path"</span>: <span class="s">"$APPDATA/reports/**"</span> }] }\n  ]\n}</pre>'),
      ],
    },
    {
      id: "ses-401", agentId: "incident-triage", title: "Разбор INC-2291", group: "Вчера",
      items: [
        u("Разбери INC-2291: что сломалось первым и какие ошибки доминируют?", "09:15"),
        steps("Запрошу логи за окно инцидента и сгруппирую по сообщению.", [
          call("query-opensearch", "· 12 480 событий", "1.4 с"),
          call("chart_render", "· 5xx по минутам", "0.3 с"),
        ], "2.8 с"),
        a("<h3>Первый сбой — 04:12:38</h3><p>Раньше всех начал падать <code>orders-api</code>: pool exhausted, затем каскад 502 на шлюзе.</p>" +
          "<table><tr><th>Сервис</th><th>Ошибка</th><th>Доля</th></tr>" +
          '<tr><td class="mono">orders-api</td><td>connection pool exhausted</td><td class="mono">64%</td></tr>' +
          '<tr><td class="mono">gateway</td><td>upstream 502</td><td class="mono">27%</td></tr>' +
          '<tr><td class="mono">billing</td><td>timeout 5s</td><td class="mono">9%</td></tr></table>' +
          "<p>График 5xx приложил к разбору; пик совпадает с деплоем 04:11.</p>"),
      ],
    },
    {
      id: "ses-501", agentId: "sla-metrics", title: "SLA за октябрь", group: "Неделя",
      items: [
        u("Посчитай SLA за октябрь и сравни с сентябрём.", "Ср"),
        steps("Возьму выгрузку из OpenSearch и посчитаю доступность по сервисам.", [
          call("query-opensearch", "· 31 день, 8 сервисов", "700 мс"),
        ], "1.9 с"),
        a("<h3>SLA · октябрь</h3><p>Целевые 99.5% выполнены везде, кроме <code>billing</code> (99.31%). Просадка — 4 и 5 октября, по 46 минут.</p>" +
          fileRow("sla-2026-10.md", "8.2 КБ")),
      ],
    },
  ];
})(window.App);
