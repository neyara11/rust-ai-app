/* Отрисовка сайдбара, топбара, ленты диалога и композера. */
(function (A) {
  "use strict";
  const { $, $$, esc, IC, S } = A;

  A.renderSidebar = function () {
    const ag = A.agent();
    const trust = A.trustState(ag);
    $("#side-av").textContent = ag.initials;
    $("#side-av").className = "av " + (ag.av || "");
    $("#side-name").textContent = ag.name;
    $("#side-sub").textContent = ag.id + " · " + trust.label;
    $("#side-dot").className = "dot " + trust.dot;
    $("#nav-count").textContent = A.AGENTS.length;
    $("#nav-tools").textContent = ag.tools.length;
    $("#nav-tools").className = "r" + (ag.trusted ? "" : " bad");
    $("#mode-label").textContent =
      (A.config.mode === "corporate" ? "корп" : "локальн") + " · " + (A.config.role === "developer" ? "разр" : "польз");

    const q = S.search.trim().toLowerCase();
    const list = A.agentSessions().filter((s) => !q || s.title.toLowerCase().includes(q));
    let html = "";
    ["Сегодня", "Вчера", "Неделя"].forEach((g) => {
      const group = list.filter((s) => s.group === g);
      if (!group.length) return;
      html += '<div class="g">' + g + "</div>";
      group.forEach((s) => {
        const cur = s.id === S.sessionId && S.view === "chat";
        html += '<a class="' + (cur ? "cur" : "") + '" data-session="' + s.id + '">' + esc(s.title) + "</a>";
      });
    });
    if (!html) html = '<div class="none">Ничего не найдено</div>';
    $("#hist").innerHTML = html;
    const fly = $("#hist-flyout-list");
    if (fly) fly.innerHTML = html;
    const hf = $("#hf-search");
    if (hf && document.activeElement !== hf) hf.value = S.search;
    const si = $("#search-input");
    if (si && document.activeElement !== si) si.value = S.search;

    $$(".nav[data-view]").forEach((b) => b.classList.toggle("on", b.dataset.view === S.view));
  };

  A.renderTopbar = function () {
    const ag = A.agent();
    const ses = A.session();
    $("#btn-back").classList.toggle("hidden", S.view === "catalog");

    const title = $("#tb-title");
    const badge = $("#tb-badge");
    const acts = $("#tb-actions");
    title.innerHTML = "";
    title.dataset.editing = "";

    if (S.view === "chat") {
      title.textContent = ses ? ses.title : "Новый диалог";
      title.title = "Переименовать сессию";
      badge.textContent = ses ? ses.id : "—";
      badge.className = "badge";
      acts.innerHTML = '<button class="ib" data-pop="pop-more" title="Действия с сессией">' + IC.more + "</button>";
    } else if (S.view === "catalog") {
      title.textContent = "Каталог агентов";
      title.title = "";
      badge.textContent = A.AGENTS.length + " " + A.plural(A.AGENTS.length, ["пакет", "пакета", "пакетов"]);
      badge.className = "badge accent";
      acts.innerHTML = '<button class="btn ghost sm" data-act="focus-install">' + IC.plus + " Установить пакет</button>";
    } else if (S.view === "tools") {
      title.textContent = "MCP-консоль";
      title.title = "";
      badge.textContent = ag.id;
      badge.className = "badge mono";
      acts.innerHTML = '<button class="btn ghost sm" data-act="focus-server">Обновить список</button>';
    } else {
      title.textContent = "Мастер пакета";
      title.title = "";
      badge.textContent = "schema 2";
      badge.className = "badge mono";
      acts.innerHTML = "";
    }
  };

  A.renderThread = function () {
    const ses = A.session();
    const ag = A.agent();
    const col = $("#thread-col");
    const thread = $("#thread");

    if (!ses || !ses.items.length) {
      col.innerHTML =
        '<div class="empty"><div class="big-av">' + esc(ag.initials) + "</div>" +
        "<h2>" + esc(ag.name) + "</h2><p>" + esc(ag.desc) + "</p>" +
        '<div class="sugg">' +
        (ag.subs || []).map((s) => '<button data-sugg="' + esc(s) + '">' + esc(s) + "</button>").join("") +
        "</div></div>";
    } else {
      col.innerHTML = ses.items.map((it, i) => A.renderItem(it, i)).join("");
    }

    A.updateComposer();
    requestAnimationFrame(() => { thread.scrollTop = thread.scrollHeight; });
  };

  A.renderItem = function (it, i) {
    switch (it.t) {
      case "user":
        return '<div class="turn you"><div class="u-msg">' + it.text + "</div>" +
          (it.files && it.files.length
            ? '<div class="u-files">' + it.files.map((f) => '<span class="u-file">' + IC.file + esc(f.name) + " · " + esc(f.size) + "</span>").join("") + "</div>"
            : "") +
          '<div class="u-meta"><span>' + esc(it.at) + "</span></div></div>";

      case "steps":
        return '<div class="turn"><details class="steps"' + (it.open ? " open" : "") + ">" +
          "<summary>" + IC.chev + "<span>Рассуждение и " + it.calls.length + ' вызова инструментов</span>' +
          '<span class="sum-tail">' + esc(it.dur || "") + "</span></summary>" +
          '<div class="step-list"><div class="step"><span class="ic think">' + IC.brain + "</span>" +
          '<span class="think-t">' + esc(it.think) + "</span></div>" +
          it.calls.map((c) =>
            '<div class="step"><span class="ic">' + (c.ok ? IC.check : IC.cross) + "</span>" +
            "<span><code>" + esc(c.name) + "</code> " + esc(c.detail) + "</span>" +
            '<span class="t">' + esc(c.time) + "</span></div>").join("") +
          "</div></details></div>";

      case "agent":
        return '<div class="turn"><div class="a-msg">' + it.html + "</div>" +
          '<div class="a-foot"><span class="acts">' +
          '<button data-act="copy" data-i="' + i + '">Копировать</button>' +
          '<button data-act="retry" data-i="' + i + '">Повторить</button></span>' +
          "<span>" + esc(it.meta || "980 → 210 tok · 58 tok/s · deepseek-v4-flash") + "</span></div></div>";

      case "agentfoot":
        return '<div class="turn" style="margin-top:-22px"><div class="a-foot"><span>' + esc(it.meta) + "</span></div></div>";

      case "ask": {
        if (it.state === "open") {
          return '<div class="turn"><div class="ask"><div class="q">' + it.q +
            "<small>" + esc(it.sub) + "</small></div>" +
            '<button class="btn" data-ask-deny="' + esc(it.id) + '">Отклонить</button>' +
            '<button class="btn warn" data-ask-allow="' + esc(it.id) + '">Разрешить</button></div></div>';
        }
        const ok = it.state === "allowed";
        return '<div class="turn"><div class="ask done' + (ok ? "" : " denied") + '">' +
          '<span class="who-ico">' + (ok ? IC.check : IC.cross) + "</span>" +
          '<div class="q">' + (ok ? "Вызов разрешён оператором" : "Вызов отклонён оператором") +
          "<small>" + esc(it.sub) + " · " + esc(it.at || A.now()) + "</small></div></div></div>";
      }

      case "typing":
        return '<div class="turn"><div class="typing"><i></i><i></i><i></i><span>' +
          esc(it.label || "агент печатает…") + "</span></div></div>";

      default:
        return "";
    }
  };

  A.updateComposer = function () {
    const ag = A.agent();
    const folder = S.folder || ag.folder;
    $("#folder-label").textContent = folder.split("\\").slice(-1)[0];
    $("#folder-label").title = folder;
    const names = { "": "Авто", low: "Быстро", medium: "Средне", high: "Глубоко" };
    $("#effort-label").textContent = names[S.effort] || "Авто";
    $("#model-label").textContent = S.model.split("/").slice(-1)[0];
    $("#engine-dot").className = "dot" + (A.config.llm_key ? "" : " warn");
    $("#c-hint").innerHTML = 'Агент видит только папку <b class="mono">' + esc(folder) + "</b> · внешняя запись " +
      (A.config.mode === "corporate" ? "заблокирована контуром" : "подтверждается вручную");
    $("#chat-text").placeholder = "Сообщение агенту «" + ag.name + "»…";
    $("#btn-send").classList.toggle("hidden", S.busy);
    $("#btn-stop").classList.toggle("hidden", !S.busy);
    A.renderAttachments();
  };

  A.renderAttachments = function () {
    $("#attachments").innerHTML = S.attachments.map((f, i) =>
      '<span class="att">' + esc(f.name) +
      '<button data-att-del="' + i + '" title="Убрать" aria-label="Убрать вложение">' + IC.cross + "</button></span>").join("");
  };

  A.renderArtifactsSheet = function () {
    const ses = A.session();
    if (!ses) return;
    const list = A.artifacts[ses.id] || [];
    $("#art-sub").textContent = ses.id + " · " + A.agent().name;
    $("#art-folder").textContent = S.folder || A.agent().folder;
    const files = list.filter((f) => !f.dump);
    const dumps = list.filter((f) => f.dump);
    $("#art-list").innerHTML = files.length
      ? files.map((f) => '<div class="artifact"><span class="nm">' + esc(f.name) + '</span><span class="sz">' + esc(f.size) +
        '</span><button class="btn ghost sm" data-act="artifact" data-file="' + esc(f.name) + '">открыть</button></div>').join("")
      : '<div class="inline-note">Файлов нет.</div>';
    $("#art-dumps").innerHTML = dumps.length
      ? dumps.map((f) => '<div class="artifact"><span class="nm">' + esc(f.name) + '</span><span class="sz">' + esc(f.size) + "</span></div>").join("")
      : '<div class="inline-note">Дампов нет.</div>';
  };
})(window.App);
