/* События интерфейса и запуск прототипа. */
(function (A) {
  "use strict";
  const { $, $$, esc } = A;

  function bindClicks() {
    document.addEventListener("click", (e) => {
      const t = e.target;

      /* выпадающая история в свёрнутом сайдбаре */
      const flyout = $("#hist-flyout");
      if (flyout && flyout.classList.contains("open") &&
          !t.closest("#hist-flyout") && !t.closest("#btn-rail-hist") && !t.closest("#search-wrap")) {
        flyout.classList.remove("open");
      }

      /* поповеры */
      const popTrigger = t.closest("[data-pop]");
      if (popTrigger) {
        const id = popTrigger.dataset.pop;
        const pop = document.getElementById(id);
        if (!pop) return;
        const willOpen = !pop.classList.contains("open");
        A.closePops();
        if (willOpen) A.openPop(id, popTrigger);
        e.stopPropagation();
        return;
      }
      if (!t.closest(".pop")) A.closePops();

      /* левая панель */
      const viewBtn = t.closest("[data-view]");
      if (viewBtn) { A.closePops(); A.setView(viewBtn.dataset.view); return; }

      const sheetBtn = t.closest("[data-sheet]");
      if (sheetBtn) { A.closePops(); A.openSheet(sheetBtn.dataset.sheet); return; }

      if (t.closest("[data-close]") || t.classList.contains("scrim")) { A.closeSheets(); return; }

      const pick = t.closest("[data-pick-agent]");
      if (pick) { A.selectAgent(pick.dataset.pickAgent); return; }

      const goto = t.closest("[data-goto]");
      if (goto) {
        A.closePops();
        A.setView("catalog");
        if (goto.dataset.goto === "install") setTimeout(() => $("#install-path").focus(), 120);
        return;
      }

      const ses = t.closest("[data-session]");
      if (ses) {
        A.closePops();
        $("#hist-flyout").classList.remove("open");
        A.selectSession(ses.dataset.session);
        return;
      }

      const openCard = t.closest("[data-open]");
      if (openCard) { A.selectAgent(openCard.dataset.open); return; }

      if (t.closest("[data-blocked]")) {
        const b = A.AGENTS.find((x) => !x.trusted);
        A.toast("Пакет заблокирован: " + (b ? (b.problem || "нарушена целостность") : "нарушена целостность"), "err");
        return;
      }

      /* папка */
      const folder = t.closest("[data-folder]");
      if (folder) {
        A.S.folder = folder.dataset.folder;
        A.closePops();
        A.updateComposer();
        A.renderAgentSheet();
        A.toast("Рабочая папка: " + A.S.folder);
        return;
      }
      if (t.closest("[data-folder-pick]")) {
        A.closePops();
        A.askText({
          title: "Рабочая папка", label: "Путь к папке",
          value: A.S.folder || A.agent().folder, ok: "Применить",
          hint: "Агент получит доступ только к этой папке.",
        }).then((v) => {
          if (!v) return;
          A.S.folder = v;
          const ag = A.agent();
          ag.folders = Array.from(new Set([v].concat(ag.folders || [])));
          A.updateComposer();
          A.renderAgentSheet();
          A.toast("Рабочая папка: " + v);
        });
        return;
      }

      /* композер: глубина и модель */
      const effort = t.closest("[data-effort]");
      if (effort) {
        A.S.effort = effort.dataset.effort;
        const s = A.session();
        if (s) s.effort = A.S.effort;
        $$("#pop-effort .it").forEach((b) => b.classList.toggle("sel", b === effort));
        A.closePops();
        A.updateComposer();
        return;
      }

      const model = t.closest("[data-model]");
      if (model) {
        A.S.model = model.dataset.model;
        A.config.llm_model = model.dataset.model;
        A.store.set("eco.config", A.config);
        $$("#pop-model .it[data-model]").forEach((b) => b.classList.toggle("sel", b === model));
        A.closePops();
        A.updateComposer();
        A.toast("Модель: " + A.S.model.split("/").slice(-1)[0]);
        return;
      }

      const sugg = t.closest("[data-sugg]");
      if (sugg) {
        const ta = $("#chat-text");
        ta.value = sugg.dataset.sugg;
        ta.style.height = "auto";
        ta.style.height = Math.min(ta.scrollHeight, 200) + "px";
        ta.focus();
        return;
      }

      /* подтверждения вызова */
      const allow = t.closest("[data-ask-allow]");
      if (allow) { A.resolveAsk(allow.dataset.askAllow, true); return; }
      const deny = t.closest("[data-ask-deny]");
      if (deny) { A.resolveAsk(deny.dataset.askDeny, false); return; }

      /* вложения */
      const attDel = t.closest("[data-att-del]");
      if (attDel) {
        A.S.attachments.splice(Number(attDel.dataset.attDel), 1);
        A.renderAttachments();
        return;
      }

      /* прочие действия */
      const act = t.closest("[data-act]");
      if (act) handleAct(act.dataset.act, act);
    });

    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        if (document.querySelector(".overlay.open [data-ask-cancel], .overlay.open [data-confirm-cancel]")) return;
        const flyout = $("#hist-flyout");
        if (flyout && flyout.classList.contains("open")) { flyout.classList.remove("open"); return; }
        A.closePops();
        if ($("#mode-overlay").classList.contains("open")) {
          if (!$("#mode-cancel").hidden) $("#mode-overlay").classList.remove("open");
          return;
        }
        A.closeSheets();
      }
      if (e.ctrlKey && e.key.toLowerCase() === "b") { e.preventDefault(); A.toggleCollapse(); }
      if (e.ctrlKey && e.key.toLowerCase() === "n") { e.preventDefault(); A.newSession(); }
      if (e.ctrlKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (A.S.collapsed) {
          $("#hist-flyout").classList.add("open");
          A.renderSidebar();
          setTimeout(() => $("#hf-search").focus(), 30);
        } else {
          $("#search-input").focus();
        }
      }
    });
  }

  function handleAct(act, el) {
    switch (act) {
      case "focus-install":
        A.setView("catalog");
        setTimeout(() => $("#install-path").focus(), 100);
        break;
      case "focus-server":
        A.setView("tools");
        break;
      case "copy": {
        const ses = A.session();
        const it = ses && ses.items[Number(el.dataset.i)];
        if (!it) break;
        const text = (it.html || "").replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(() => A.toast("Текст скопирован")).catch(() => A.toast("Копирование недоступно", "warn"));
        } else {
          A.toast("Копирование недоступно в этом окружении", "warn");
        }
        break;
      }
      case "retry": {
        const ses = A.session();
        const lastUser = ses && ses.items.slice().reverse().find((i) => i.t === "user");
        if (lastUser) {
          $("#chat-text").value = lastUser.text.replace(/<[^>]+>/g, "");
          $("#chat-text").focus();
          A.toast("Запрос подставлен в поле ввода");
        }
        break;
      }
      case "artifact":
        A.toast("Файл " + (el.dataset.file || "") + " открыт быстрым просмотром");
        break;
      case "mcp-override":
        A.askText({
          title: "Переопределение MCP",
          label: "URL для этого приложения",
          value: (A.config.mcp.find((m) => m.id === el.dataset.id) || {}).url || "",
          ok: "Сохранить",
        }).then((v) => {
          if (v) { A.toast("Override для " + el.dataset.id + " сохранён"); A.renderAgentSheet(); }
        });
        break;
      case "mcp-reset":
        A.toast("Переопределение сброшено на глобальный реестр");
        break;
      case "mcp-add-app": {
        const inp = $("#app-mcp-new");
        if (!inp || !inp.value.trim()) { A.toast("Укажите id подключения", "err"); break; }
        const id = inp.value.trim();
        const ag = A.agent();
        if (!ag.connections.includes(id)) ag.connections.push(id);
        A.renderAgentSheet();
        A.renderTools();
        A.toast("Подключение " + id + " добавлено к агенту");
        break;
      }
      case "appsecret-add": {
        const n = $("#app-secret-name"), v = $("#app-secret-value");
        if (!n || !n.value.trim() || !v.value) { A.toast("Нужно имя и значение", "err"); break; }
        (A.appSecrets[A.agent().id] = A.appSecrets[A.agent().id] || []).push({ name: n.value.trim(), note: "DPAPI · зашифрован" });
        A.renderAgentSheet();
        A.toast("Секрет приложения сохранён");
        break;
      }
      case "del-appsecret":
        A.appSecrets[A.agent().id] = (A.appSecrets[A.agent().id] || []).filter((s) => s.name !== el.dataset.name);
        A.renderAgentSheet();
        A.toast("Секрет удалён", "warn");
        break;
      case "del-mcp-global":
        A.config.mcp = A.config.mcp.filter((m) => m.id !== el.dataset.id);
        A.applyConfig();
        A.toast("MCP-сервер удалён", "warn");
        break;
      case "del-secret":
        A.config.secrets = A.config.secrets.filter((s) => s.name !== el.dataset.name);
        A.applyConfig();
        A.toast("Секрет удалён", "warn");
        break;
      case "del-mcp":
        A.W.mcp = A.W.mcp.filter((m) => m.id !== el.dataset.id);
        A.renderWizard();
        break;
      default: break;
    }
  }

  function bindControls() {
    $("#btn-theme").onclick = () => A.setTheme(A.S.theme === "dark" ? "light" : "dark");
    $("#btn-collapse").onclick = A.toggleCollapse;
    $("#btn-new").onclick = A.newSession;
    $("#btn-back").onclick = () => A.setView("catalog");
    $("#btn-send").onclick = A.send;
    $("#btn-stop").onclick = A.stopGeneration;
    $("#btn-attach").onclick = () => $("#file-input").click();

    $("#file-input").onchange = (e) => {
      Array.from(e.target.files || []).forEach((f) => A.S.attachments.push({ name: f.name, size: A.fmtSize(f.size) }));
      e.target.value = "";
      A.renderAttachments();
      $("#chat-text").focus();
    };

    const ta = $("#chat-text");
    const autoGrow = () => { ta.style.height = "auto"; ta.style.height = Math.min(ta.scrollHeight, 200) + "px"; };
    ta.addEventListener("input", autoGrow);
    ta.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); A.send(); }
    });

    $("#search-input").addEventListener("input", (e) => { A.S.search = e.target.value; A.renderSidebar(); });

    /* история в свёрнутом сайдбаре */
    $("#search-wrap").addEventListener("click", () => {
      if (!A.S.collapsed) return;
      const fly = $("#hist-flyout");
      if (fly.classList.contains("open")) fly.classList.remove("open");
      else $("#btn-rail-hist").click();
    });
    $("#btn-rail-hist").onclick = () => {
      const fly = $("#hist-flyout");
      const open = !fly.classList.contains("open");
      fly.classList.toggle("open", open);
      if (open) {
        A.renderSidebar();
        setTimeout(() => $("#hf-search").focus(), 30);
      }
    };
    $("#hf-close").onclick = () => {
      $("#hist-flyout").classList.remove("open");
      $("#btn-rail-hist").focus();
    };
    $("#hf-search").addEventListener("input", (e) => {
      A.S.search = e.target.value;
      A.renderSidebar();
    });

    /* переименование сессии */
    $("#tb-title").addEventListener("click", () => {
      if (A.S.view !== "chat") return;
      const ses = A.session();
      if (!ses) return;
      const el = $("#tb-title");
      if (el.dataset.editing === "1") return;
      el.dataset.editing = "1";
      el.innerHTML = '<input id="tb-rename" value="' + esc(ses.title) + '" />';
      const input = $("#tb-rename");
      input.focus();
      input.select();
      const commit = (save) => {
        if (el.dataset.editing !== "1") return;
        el.dataset.editing = "";
        if (save && input.value.trim()) ses.title = input.value.trim();
        A.renderTopbar();
        A.renderSidebar();
      };
      input.addEventListener("keydown", (e) => {
        if (e.key === "Enter") { e.preventDefault(); commit(true); }
        if (e.key === "Escape") { e.preventDefault(); commit(false); }
      });
      input.addEventListener("blur", () => commit(true));
    });

    /* ── каталог ── */
    $("#install-btn").onclick = () => A.installPackage($("#install-path").value.trim());
    $("#install-path").addEventListener("keydown", (e) => { if (e.key === "Enter") A.installPackage($("#install-path").value.trim()); });
    $("#install-zip").onclick = () => {
      $("#install-path").value = "D:\\dist\\sla-metrics-0.2.3.zip";
      $("#install-path").focus();
      A.toast("Выбор файла недоступен в прототипе — путь подставлен", "warn");
    };
    $("#install-folder").onclick = () => { $("#install-path").value = "D:\\apps\\doc-writer"; $("#install-path").focus(); };
    $$("[data-quick]").forEach((b) => {
      b.onclick = () => { $("#install-path").value = b.dataset.quick; A.installPackage(b.dataset.quick, true); };
    });

    /* ── MCP-консоль ── */
    $("#server-select").addEventListener("change", () => {
      A.fillToolSelect();
      $("#tool-args").value = A.sampleArgs($("#tool-select").value);
    });
    $("#tool-select").addEventListener("change", () => {
      $("#tool-args").value = A.sampleArgs($("#tool-select").value);
    });
    $("#tool-call").onclick = A.runToolCall;
    $("#tool-sample").onclick = () => {
      $("#tool-args").value = A.sampleArgs($("#tool-select").value);
      A.toast("Шаблон аргументов подставлен");
    };

    /* ── мастер пакета ── */
    $("#pack-files").addEventListener("click", (e) => {
      const f = e.target.closest("[data-file]");
      if (f) A.loadFile(f.dataset.file);
    });
    $("#wiz-role-enable").onclick = A.toggleRole;
    $("#pack-editor").addEventListener("input", () => { A.W.files[A.W.current].content = $("#pack-editor").value; });
    $("#pack-save").onclick = () => {
      A.W.files[A.W.current].content = $("#pack-editor").value;
      A.renderWizard();
      A.toast("Файл сохранён: " + A.W.current);
    };
    $("#pack-new").onclick = () => {
      const name = $("#pack-new-file").value.trim();
      if (!name) { A.toast("Укажите имя файла", "err"); return; }
      A.W.files[name] = { content: "" };
      $("#pack-new-file").value = "";
      A.renderWizard();
      A.loadFile(name);
      A.toast("Создан файл " + name);
    };
    $("#pack-pick").onclick = () => {
      $("#pack-dir").value = "D:\\apps\\doc-writer";
      A.toast("Выбор папки недоступен в прототипе", "warn");
    };    $("#pack-load").onclick = () => {
      A.W.dir = $("#pack-dir").value.trim() || A.W.dir;
      A.wlog("загружен пакет: " + A.W.dir);
      A.toast("Пакет загружен");
    };
    $("#pack-import").onclick = () => {
      A.W.appId = $("#pack-app-id").value.trim() || A.W.appId;
      A.W.name = $("#pack-name").value.trim() || A.W.name;
      A.wlog("scan AGENTS.md → manifest.json");
      A.wlog("manifest.json собран из исходников", "ok");
      A.toast("Манифест собран из исходников");
    };
    $("#pack-init").onclick = () => {
      A.wlog("создан каркас пакета (" + $("#pack-template").value + ")", "ok");
      A.toast("Каркас создан");
    };
    $("#pack-apply-template").onclick = () => {
      A.W.template = $("#pack-template").value;
      A.wlog("шаблон " + A.W.template + " применён к манифесту", "ok");
      A.toast("Шаблон применён");
    };
    $("#pack-validate").onclick = () => {
      A.wlog("eco-pack validate --schema 2");
      setTimeout(() => {
        A.wlog("схема ок · allow-list ок · версия ок", "ok");
        $("#pack-status").className = "badge ok";
        $("#pack-status").textContent = "eco-pack: проверка пройдена";
        A.toast("Пакет прошёл проверку");
      }, 500);
    };
    $("#pack-build").onclick = () => {
      A.wlog("eco-pack build --profile prod");
      setTimeout(() => {
        A.wlog("zip собран: dist/" + A.W.appId + "-0.1.0.zip · 42 КБ", "ok");
        A.toast("Пакет собран");
      }, 600);
    };
    $("#pack-sign").onclick = () => {
      const key = $("#pack-key").value.trim() || "D:\\keys\\me\\dev.key";
      A.wlog("eco-pack sign --key " + key);
      setTimeout(() => {
        A.W.signed = true;
        A.wlog("подпись dev: 3f7b20c9… (только локальный тест)", "ok");
        A.toast("Пакет подписан dev-ключом");
      }, 500);
    };
    $("#pack-keygen").onclick = () => {
      $("#pack-key").value = "D:\\keys\\me\\dev.key";
      A.wlog("создан dev-ключ D:\\keys\\me\\dev.key", "ok");
      A.toast("Dev-ключ создан");
    };
    $("#pack-mcp-add").onclick = () => {
      const id = $("#pack-mcp-id").value.trim();
      if (!id) { A.toast("Укажите id подключения", "err"); return; }
      if (A.W.mcp.some((m) => m.id === id)) { A.toast("Такое подключение уже есть", "warn"); return; }
      A.W.mcp.push({ id, write: $("#pack-mcp-write").checked });
      $("#pack-mcp-id").value = "";
      A.renderWizard();
      A.wlog("в манифест добавлено подключение " + id, "ok");
      A.toast("MCP-подключение добавлено");
    };
    $("#pack-install").onclick = () => {
      if (!A.W.signed && A.config.mode === "corporate") { A.toast("Сначала подпишите пакет dev-ключом", "err"); return; }
      const id = (A.W.appId || "my-app").toLowerCase().replace(/[^a-z0-9-]/g, "-");
      if (A.AGENTS.some((x) => x.id === id)) { A.toast("Пакет с таким app_id уже установлен", "warn"); return; }
      A.AGENTS.push({
        id, name: A.W.name || id, initials: (A.W.name || id).slice(0, 2).toUpperCase(), av: "", ver: "0.1.0",
        publisher: A.W.signed ? "CN=ECO AIOps Dev" : "локальный слепок", trusted: true, seal: true, allow: true,
        sealHash: A.W.signed ? "3f7b20c9a1" : "local:3f7b20",
        folder: "D:\\reports\\" + id, folders: ["D:\\reports\\" + id],
        desc: "Пакет собран в мастере и установлен в контур.",
        tools: [{ name: "platform_list_files", kind: "read" }, { name: "platform_write_file", kind: "write" }],
        connections: ["llm"], subs: ["Что лежит в папке?", "Сделай выжимку"],
      });
      A.SESSIONS.unshift({ id: A.nextSessionId(), agentId: id, title: "Новый диалог", group: "Сегодня", items: [], folder: "D:\\reports\\" + id });
      A.wlog("пакет установлен в контур", "ok");
      A.toast("Пакет установлен");
      A.selectAgent(id);
    };

    /* ── настройки ── */
    $("#set-role-toggle").onclick = A.toggleRole;
    $("#set-mode-change").onclick = () => A.openMode(false);
    $("#set-probe").onclick = () => {
      const out = $("#set-probe-out");
      out.className = "badge";
      out.textContent = "проверка…";
      setTimeout(() => {
        out.className = "badge ok";
        out.textContent = "доступен · 12 мс";
        A.toast("Шлюз доступен");
      }, 700);
    };
    $("#set-key-save").onclick = () => {
      if (!$("#set-llm-key").value.trim()) { A.toast("Введите ключ", "err"); return; }
      A.config.llm_key = true;
      $("#set-llm-key").value = "";
      A.applyConfig();
      A.toast("Ключ зашифрован через DPAPI");
    };
    $("#set-key-del").onclick = () => {
      A.config.llm_key = false;
      A.applyConfig();
      A.toast("Ключ удалён из DPAPI", "warn");
    };
    $("#set-mcp-save").onclick = () => {
      const id = $("#set-mcp-id").value.trim();
      const url = $("#set-mcp-url").value.trim();
      if (!id || !url) { A.toast("Нужны id и endpoint", "err"); return; }
      const existing = A.config.mcp.find((m) => m.id === id);
      if (existing) { existing.url = url; existing.tls = !$("#set-mcp-insecure").checked; }
      else A.config.mcp.push({ id, url, tls: !$("#set-mcp-insecure").checked });
      $("#set-mcp-id").value = "";
      $("#set-mcp-url").value = "";
      $("#set-mcp-token").value = "";
      A.applyConfig();
      A.toast("MCP-сервер сохранён");
    };
    $("#set-secret-save").onclick = () => {
      const name = $("#set-secret-name").value.trim();
      const val = $("#set-secret-value").value;
      if (!name || !val) { A.toast("Нужно имя и значение", "err"); return; }
      A.config.secrets = A.config.secrets.filter((s) => s.name !== name);
      A.config.secrets.push({ name, note: "DPAPI · зашифрован" });
      $("#set-secret-name").value = "";
      $("#set-secret-value").value = "";
      A.applyConfig();
      A.toast("Секрет сохранён в DPAPI");
    };
    $("#set-art-app").onchange = () => { A.fillArtSessions(); A.renderArtList(); };
    $("#set-art-session").onchange = () => A.renderArtList();
    $("#set-art-search").onclick = () => { A.renderArtList(); A.toast("Поиск по дампам выполнен"); };
    $("#set-art-export").onclick = () => A.toast("Дампы выгружены в папку профиля");
    $("#set-art-cleanup").onclick = async () => {
      const ok = await A.confirm({
        title: "Очистить дампы?",
        text: "Будут удалены tool-call дампы всех приложений. Файлы и диалоги останутся.",
        ok: "Очистить",
      });
      if (!ok) return;
      Object.keys(A.artifacts).forEach((k) => { A.artifacts[k] = A.artifacts[k].filter((f) => !f.dump); });
      A.renderArtList();
      $("#set-art-usage").textContent = "0 МБ / " + Math.round(A.config.q_global / 1024) + " ГБ";
      A.toast("Дампы очищены", "warn");
    };
    $("#set-save").onclick = () => {
      A.readSettings();
      A.applyConfig();
      $("#set-saved-at").textContent = "сохранено в " + A.now();
      A.toast("Конфигурация сохранена");
    };
    $("#set-reset").onclick = async () => {
      const ok = await A.confirm({
        title: "Сбросить конфигурацию?",
        text: "Endpoint, модель, MCP-серверы и лимиты вернутся к значениям по умолчанию.",
        ok: "Сбросить",
      });
      if (!ok) return;
      A.config = JSON.parse(JSON.stringify(A.DEFAULT_CONFIG));
      A.store.set("eco.config", A.config);
      A.applyConfig();
      A.toast("Конфигурация сброшена", "warn");
    };

    /* ── шторка агента ── */
    $("#sheet-agent-tabs").addEventListener("click", (e) => {
      const tab = e.target.closest("[data-atab]");
      if (!tab) return;
      A.S.dialogTab = tab.dataset.atab;
      A.renderAgentSheet();
    });
    $("#agent-open-catalog").onclick = () => { A.closeSheets(); A.setView("catalog"); };
    $("#agent-open-console").onclick = () => { A.closeSheets(); A.setView("tools"); };

    /* ── артефакты сессии ── */
    $("#art-export").onclick = () => A.toast("Артефакты выгружены");
    $("#art-clean").onclick = () => {
      A.artifacts[A.S.sessionId] = [];
      A.renderArtifactsSheet();
      A.toast("Дампы сессии очищены", "warn");
    };

    /* ── режим ── */
    $("#mode-grid").addEventListener("click", (e) => {
      const c = e.target.closest("[data-mode]");
      if (!c) return;
      A.setPendingMode(c.dataset.mode);
    });
    $$('input[name="mode-role"]').forEach((r) => r.addEventListener("change", () => A.setPendingRole(r.value)));
    $("#mode-confirm").onclick = A.confirmMode;
    $("#mode-cancel").onclick = () => $("#mode-overlay").classList.remove("open");

    /* ── попап «ещё» ── */
    $("#mi-rename").onclick = () => { A.closePops(); $("#tb-title").click(); };
    $("#mi-export").onclick = () => { A.closePops(); A.exportThread(); };
    $("#mi-artifacts").onclick = () => { A.closePops(); A.openSheet("sheet-artifacts"); };
    $("#mi-end").onclick = () => {
      A.closePops();
      if (A.S.busy) { A.clearTimers(); A.S.busy = false; }
      A.setView("catalog");
      A.toast("Сессия завершена");
    };
    $("#mi-delete").onclick = async () => {
      A.closePops();
      const ses = A.session();
      if (!ses) return;
      const ok = await A.confirm({
        title: "Удалить сессию?",
        text: "Диалог «" + ses.title + "» и его временные артефакты будут удалены без возможности восстановления.",
        ok: "Удалить",
      });
      if (!ok) return;
      A.SESSIONS = A.SESSIONS.filter((s) => s.id !== ses.id);
      delete A.artifacts[ses.id];
      let next = A.SESSIONS.find((s) => s.agentId === A.S.agentId) || A.SESSIONS[0];
      if (!next) {
        next = {
          id: A.nextSessionId(), agentId: A.S.agentId, title: "Новый диалог",
          group: "Сегодня", items: [], folder: A.S.folder,
        };
        A.SESSIONS.unshift(next);
      }
      A.selectSession(next.id);
      A.toast("Сессия удалена", "warn");
    };

    /* Enter на карточке каталога */
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      const card = document.activeElement && document.activeElement.closest("[data-open]");
      if (card) A.selectAgent(card.dataset.open);
    });
  }

  let started = false;
  function init() {
    if (started) return;
    started = true;
    A.S.collapsed = A.store.get("eco.collapsed", false);
    $("#app").classList.toggle("collapsed", A.S.collapsed);
    A.setTheme(document.documentElement.dataset.theme || "dark");

    const first = A.AGENTS[0];
    A.S.folder = first.folder;
    A.S.model = A.config.llm_model;

    bindClicks();
    bindControls();

    A.renderSidebar();
    A.renderTopbar();
    A.renderThread();
    A.renderCatalog();
    A.fillSettings();

    if (!A.S.modeChosen) {
      setTimeout(() => A.openMode(true), 220);
    }
  }

  document.addEventListener("DOMContentLoaded", init);
  if (document.readyState !== "loading") init();
})(window.App);
