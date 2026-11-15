/* Выбор режима контура: первый запуск и смена из настроек. */
(function (A) {
  "use strict";
  const { $, $$, esc } = A;

  let pendingMode = A.config.mode;
  let pendingRole = A.config.role;

  A.syncModeDialog = function () {
    $$("#mode-grid .mode-card").forEach((c) => c.classList.toggle("on", c.dataset.mode === pendingMode));
    $$('input[name="mode-role"]').forEach((r) => (r.checked = r.value === pendingRole));
    const names = { corporate: "корпоративный", local: "локальный" };
    $("#mode-note").textContent = "Выбрано: " + names[pendingMode] + " · " + (pendingRole === "developer" ? "разработчик" : "пользователь");
  };

  A.setPendingMode = function (m) { pendingMode = m; A.syncModeDialog(); };
  A.setPendingRole = function (r) { pendingRole = r; A.syncModeDialog(); };

  A.openMode = function (first) {
    pendingMode = A.config.mode;
    pendingRole = A.config.role;
    A.syncModeDialog();
    $("#mode-cancel").hidden = !!first;
    const eyebrow = document.querySelector("#mode-overlay .eyebrow");
    if (eyebrow) eyebrow.textContent = first ? "Первый запуск" : "Смена режима";
    $("#mode-note").textContent = first ? "Режим можно сменить позже в настройках." : "Выбрано: " +
      (pendingMode === "corporate" ? "корпоративный" : "локальный") + " · " + (pendingRole === "developer" ? "разработчик" : "пользователь");
    $("#mode-overlay").classList.add("open");
  };

  A.confirmMode = function () {
    A.config.mode = pendingMode;
    A.config.role = pendingRole;
    A.S.modeChosen = true;
    A.store.set("eco.modeChosen", true);
    A.store.set("eco.config", A.config);
    $("#mode-overlay").classList.remove("open");
    A.applyConfig();
    A.toast("Режим: " + (pendingMode === "corporate" ? "корпоративный" : "локальный") +
      " · роль " + (pendingRole === "developer" ? "разработчик" : "пользователь"));
  };
})(window.App);
