/* Ядро прототипа: утилиты, состояние, навигация, шторки, поповеры, тосты. */
window.App = window.App || {};

(function (A) {
  "use strict";

  A.$ = (s, r) => (r || document).querySelector(s);
  A.$$ = (s, r) => Array.from((r || document).querySelectorAll(s));

  A.esc = (s) => String(s == null ? "" : s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
  A.now = () => new Date().toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
  A.uid = (p) => p + "-" + Math.random().toString(36).slice(2, 6);

  A.store = {
    get(k, d) { try { const v = localStorage.getItem(k); return v === null ? d : JSON.parse(v); } catch (e) { return d; } },
    set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch (e) {} },
  };

  A.fmtSize = (n) => (n < 1024 ? n + " Б" : (n / 1024).toFixed(1) + " КБ");

  A.plural = function (n, forms) {
    const a = Math.abs(n) % 100, b = a % 10;
    if (a > 10 && a < 20) return forms[2];
    if (b > 1 && b < 5) return forms[1];
    if (b === 1) return forms[0];
    return forms[2];
  };

  /* Единая семантика доверия: подпись, локальный seal и блок — разные состояния. */
  A.trustState = function (ag) {
    if (!ag || !ag.trusted) return { label: "заблокирован", badge: "блок", cls: "bad", dot: "bad" };
    if (/локальн/i.test(ag.publisher || "")) return { label: "локальный seal", badge: "локальный seal", cls: "warn", dot: "warn" };
    return { label: "подпись ок", badge: "подпись ок", cls: "ok", dot: "" };
  };

  /* Политика внешней записи зависит от режима, а не от конкретного экрана. */
  A.writePolicy = function () {
    return A.config && A.config.mode === "corporate"
      ? { label: "БЛОК", cls: "bad" }
      : { label: "подтверждать", cls: "warn" };
  };

  A.IC = {
    chev: '<svg class="chev" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M9 6l6 6-6 6"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6L9 17l-5-5"/></svg>',
    cross: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M18 6L6 18M6 6l12 12"/></svg>',
    warn: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l9 17H3L12 3z"/><path d="M12 9v5M12 17.4v.2"/></svg>',
    brain: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="12" cy="12" r="3"/><path d="M12 3v3M12 18v3M3 12h3M18 12h3"/></svg>',
    file: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>',
    more: '<svg viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg>',
    plus: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>',
    arrow: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg>',
  };

  /* ── состояние ─────────────────────────────────────────── */
  A.S = {
    theme: document.documentElement.dataset.theme || "dark",
    view: "chat",
    agentId: "doc-writer",
    sessionId: "ses-301",
    folder: "",
    effort: "medium",
    model: "~deepseek/deepseek-v4-flash-latest",
    attachments: [],
    busy: false,
    dialogTab: "access",
    search: "",
    collapsed: false,
    modeChosen: A.store.get("eco.modeChosen", false),
    timers: [],
  };

  /* ── тосты ─────────────────────────────────────────────── */
  A.toast = function (text, kind) {
    const el = document.createElement("div");
    el.className = "toast " + (kind || "");
    el.setAttribute("role", kind === "err" ? "alert" : "status");
    const icon = kind === "err" ? A.IC.cross : kind === "warn" ? A.IC.warn : A.IC.check;
    el.innerHTML = '<span class="i">' + icon + "</span><span>" + A.esc(text) + "</span>";
    A.$("#toasts").appendChild(el);
    setTimeout(() => { el.classList.add("out"); setTimeout(() => el.remove(), 220); }, 3200);
  };

  /* ── диалог подтверждения ──────────────────────────────── */
  A.confirm = function (opts) {
    return new Promise((resolve) => {
      const wrap = document.createElement("div");
      wrap.className = "overlay open";
      wrap.style.zIndex = "90";
      wrap.innerHTML =
        '<div class="dialog" style="max-width:430px">' +
        '<h1 style="font-size:18px;margin-bottom:8px">' + A.esc(opts.title) + "</h1>" +
        '<p class="lead" style="margin:0 0 16px">' + A.esc(opts.text || "") + "</p>" +
        '<div class="dialog-foot"><span class="sp"></span>' +
        '<button class="btn ghost sm" data-confirm-cancel>Отмена</button>' +
        '<button class="btn ' + (opts.danger === false ? "pri" : "danger") + ' sm" data-confirm-ok>' +
        A.esc(opts.ok || "Удалить") + "</button></div></div>";
      document.body.appendChild(wrap);
      const done = (val) => { wrap.remove(); document.removeEventListener("keydown", onKey); resolve(val); };
      const onKey = (e) => { if (e.key === "Escape") { e.preventDefault(); done(false); } };
      wrap.querySelector("[data-confirm-cancel]").onclick = () => done(false);
      wrap.querySelector("[data-confirm-ok]").onclick = () => done(true);
      wrap.onclick = (e) => { if (e.target === wrap) done(false); };
      document.addEventListener("keydown", onKey);
      setTimeout(() => wrap.querySelector("[data-confirm-cancel]").focus(), 30);
    });
  };

  /* ── диалог ввода ──────────────────────────────────────── */
  A.askText = function (opts) {
    return new Promise((resolve) => {
      const wrap = document.createElement("div");
      wrap.className = "overlay open";
      wrap.style.zIndex = "85";
      wrap.innerHTML =
        '<div class="dialog" style="max-width:430px">' +
        '<h1 style="font-size:18px;margin-bottom:10px">' + A.esc(opts.title) + "</h1>" +
        '<label class="field">' + A.esc(opts.label || "") +
        '<input id="ask-input" value="' + A.esc(opts.value || "") + '" placeholder="' + A.esc(opts.placeholder || "") + '" /></label>' +
        (opts.hint ? '<p class="inline-note">' + A.esc(opts.hint) + "</p>" : "") +
        '<div class="dialog-foot" style="margin-top:14px"><span class="sp"></span>' +
        '<button class="btn ghost sm" data-ask-cancel>Отмена</button>' +
        '<button class="btn pri sm" data-ask-ok>' + A.esc(opts.ok || "Готово") + "</button></div></div>";
      document.body.appendChild(wrap);
      const input = wrap.querySelector("#ask-input");
      const done = (val) => { wrap.remove(); document.removeEventListener("keydown", onKey); resolve(val); };
      const onKey = (e) => {
        if (e.key === "Escape") { e.preventDefault(); done(null); }
        if (e.key === "Enter" && document.activeElement === input) { e.preventDefault(); done(input.value.trim()); }
      };
      wrap.querySelector("[data-ask-cancel]").onclick = () => done(null);
      wrap.querySelector("[data-ask-ok]").onclick = () => done(input.value.trim());
      wrap.onclick = (e) => { if (e.target === wrap) done(null); };
      document.addEventListener("keydown", onKey);
      setTimeout(() => { input.focus(); input.select(); }, 30);
    });
  };

  /* ── поповеры ──────────────────────────────────────────── */
  A.closePops = function () { A.$$(".pop.open").forEach((p) => p.classList.remove("open")); };

  A.openPop = function (id, trigger) {
    const pop = document.getElementById(id);
    if (!pop) return;
    if (id === "pop-agent" && A.renderAgentPop) A.renderAgentPop();
    if (id === "pop-folder" && A.renderFolderPop) A.renderFolderPop();
    A.$$(".pop.open").forEach((p) => { if (p !== pop) p.classList.remove("open"); });
    pop.classList.add("open");
    pop.style.left = "0px"; pop.style.top = "0px";
    const r = trigger.getBoundingClientRect();
    const w = pop.offsetWidth, h = pop.offsetHeight;
    let left = Math.max(12, Math.min(r.left, innerWidth - w - 12));
    let top = r.bottom + 6;
    if ((trigger.dataset.place || "below") === "above") top = r.top - h - 6;
    top = Math.max(12, Math.min(top, innerHeight - h - 12));
    pop.style.left = left + "px";
    pop.style.top = top + "px";
  };

  /* ── шторки ────────────────────────────────────────────── */
  A.openSheet = function (id) {
    A.closePops();
    A.$$(".sheet.open").forEach((s) => s.classList.remove("open"));
    const el = document.getElementById(id);
    if (!el) return;
    if (id === "sheet-agent" && A.renderAgentSheet) A.renderAgentSheet();
    if (id === "sheet-settings" && A.fillSettings) A.fillSettings();
    if (id === "sheet-artifacts" && A.renderArtifactsSheet) A.renderArtifactsSheet();
    el.classList.add("open");
    A.$(".scrim").classList.add("open");
  };

  A.closeSheets = function () {
    A.$$(".sheet.open").forEach((s) => s.classList.remove("open"));
    A.$(".scrim").classList.remove("open");
  };

  /* ── тема и свёрнутость ────────────────────────────────── */
  A.setTheme = function (t) {
    A.S.theme = t;
    document.documentElement.dataset.theme = t;
    A.store.set("eco.theme", t);
    if (A.updateComposer) A.updateComposer();
  };

  A.toggleCollapse = function () {
    A.S.collapsed = !A.S.collapsed;
    A.$("#app").classList.toggle("collapsed", A.S.collapsed);
    if (!A.S.collapsed) {
      const fly = A.$("#hist-flyout");
      if (fly) fly.classList.remove("open");
    }
    A.store.set("eco.collapsed", A.S.collapsed);
  };

  /* ── вид ───────────────────────────────────────────────── */
  A.setView = function (v) {
    A.S.view = v;
    A.$$(".view").forEach((el) => el.classList.toggle("on", el.id === "view-" + v));
    if (A.renderTopbar) A.renderTopbar();
    if (A.renderSidebar) A.renderSidebar();
    if (v === "catalog" && A.renderCatalog) A.renderCatalog();
    if (v === "tools" && A.renderTools) A.renderTools();
    if (v === "wizard" && A.renderWizard) A.renderWizard();
    if (v === "chat" && A.renderThread) {
      A.renderThread();
      requestAnimationFrame(() => A.$("#chat-text").focus());
    }
  };
})(window.App);
