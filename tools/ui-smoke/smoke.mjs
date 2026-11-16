import { createServer } from "node:http";
import { mkdir, readFile } from "node:fs/promises";
import { extname, isAbsolute, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const ROOT = fileURLToPath(new URL("../../src/", import.meta.url));
const APP_JS = fileURLToPath(new URL("../../src/app.js", import.meta.url));
const SHOT_DIR = process.env.UI_SMOKE_SHOTS || "C:\\Users\\pechn\\AppData\\Local\\Temp\\kilo";
const appSource = await readFile(APP_JS, "utf8");
const REQUIRED_IDS = [
  ...new Set(
    [...appSource.matchAll(/\$\("([^"]+)"\)/g)].map((match) => match[1]),
  ),
];
await mkdir(SHOT_DIR, { recursive: true });
const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".woff2": "font/woff2",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const server = createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
  const file = normalize(join(ROOT, path === "/" ? "index.html" : path));
  const rel = relative(ROOT, file);
  if (rel.startsWith("..") || isAbsolute(rel)) {
    response.writeHead(403).end("forbidden");
    return;
  }
  try {
    const data = await readFile(file);
    response.writeHead(200, { "content-type": MIME[extname(file)] || "application/octet-stream" });
    response.end(data);
  } catch {
    response.writeHead(404).end("not found");
  }
});

await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const pageErrors = [];
const externalRequests = [];
page.on("pageerror", (error) => pageErrors.push(error));
page.on("request", (request) => {
  const url = request.url();
  if (!url.startsWith("http://127.0.0.1") && !url.startsWith("http://localhost")) {
    externalRequests.push(url);
  }
});

try {
  await page.goto(`http://127.0.0.1:${port}/index.html`);

  const missingIds = await page.evaluate(
    (ids) => ids.filter((id) => !document.getElementById(id)),
    REQUIRED_IDS,
  );
  assert(
    missingIds.length === 0,
    `id contract broken: app.js queries ${missingIds.length} missing id(s): ${missingIds.join(", ")}`,
  );
  await page.waitForSelector("#apps-panel .card");

  const appCount = await page.locator("#apps-panel .card").count();
  assert(appCount === 3, `expected 3 mock apps, got ${appCount}`);

  const trusted = page.locator('[data-open="doc-writer"]');
  assert(await trusted.isEnabled(), "trusted app must be openable");
  const untrusted = page.locator('[data-open="incident-brief"]');
  assert(!(await untrusted.isEnabled()), "untrusted app must be disabled");

  const allowBadge = await page.locator("#allowlist").textContent();
  assert(allowBadge.includes("allowlist: нет"), `unexpected allowlist badge: ${allowBadge}`);

  await trusted.click();
  await page.waitForSelector("#workspace:not([hidden])");
  await page.waitForFunction(
    () => document.querySelector("#chat-log")?.textContent.includes("report.md"),
  );
  await page.waitForFunction(
    () => document.querySelector("#chat-log")?.textContent.includes("report.docx"),
  );
  await page.waitForSelector('.diagram-block[data-diagram="mermaid"] svg', { timeout: 15000 });
  await page.waitForSelector('.diagram-block[data-diagram="chart"] canvas', { timeout: 15000 });
  assert(
    (await page.locator("#chat-log .md-table-wrap table").count()) === 1,
    "markdown table was not rendered",
  );
  assert(
    (await page.locator("#chat-log .md-code [data-copy-code]").count()) >= 1,
    "code block toolbar was not rendered",
  );
  assert(
    (await page.locator('#chat-log .md-code code[class*="language-sql"]').count()) === 1,
    "code block lost its language class",
  );
  assert(
    (await page.locator("#chat-log .md-code code.hljs .hljs-keyword").count()) >= 1,
    "code block was not syntax highlighted",
  );
  const fileChips = page.locator("#chat-log .ws-file");
  assert((await fileChips.count()) >= 2, "workspace file chips were not rendered");
  assert(
    (await page.locator('#chat-log .ws-file-name[data-ws-file="report.md"]').count()) >= 1,
    "report.md chip is missing",
  );
  assert(
    (await page.locator('#chat-log .ws-file-name[data-ws-file="docx_export"]').count()) === 0,
    "tool names must not become file chips",
  );
  await page.locator('#chat-log .ws-file-name[data-ws-file="report.md"]').first().click();
  await page.waitForFunction(
    () => document.querySelector("#toast")?.textContent.includes("Открываю report.md"),
    null,
    { timeout: 5000 },
  );
  await page.locator('#chat-log [data-ws-file="report.md"][data-ws-action="save"]').first().click();
  await page.waitForFunction(
    () => document.querySelector("#toast")?.textContent.includes("Копия сохранена"),
    null,
    { timeout: 5000 },
  );
  await page.click("#ws-folder-open");
  await page.waitForFunction(
    () => document.querySelector("#toast")?.textContent.includes("Открываю рабочую папку"),
    null,
    { timeout: 5000 },
  );
  await page.waitForFunction(
    () => {
      const diagrams = [...document.querySelectorAll('.diagram-block[data-diagram="mermaid"]')];
      return (
        diagrams.length >= 2 &&
        diagrams.every((block) => block.querySelector("svg")) &&
        !document.querySelector(".diagram-error")
      );
    },
    null,
    { timeout: 15000 },
  );
  assert(
    (await page.locator('.diagram-block[data-diagram="chart"] [data-diagram-action="svg"]').count()) === 0,
    "Chart.js must not offer a vector SVG export",
  );
  await page.click('.diagram-block[data-diagram="mermaid"] [data-diagram-action="svg"]');
  await page.waitForFunction(
    () => document.querySelector("#toast")?.textContent.includes(".svg"),
    null,
    { timeout: 5000 },
  );
  await page.click('.diagram-block[data-diagram="chart"] [data-diagram-action="png"]');
  await page.waitForFunction(
    () => document.querySelector("#toast")?.textContent.includes(".png"),
    null,
    { timeout: 5000 },
  );

  const stats = await page.locator("#chat-log").textContent();
  assert(stats.includes("58.4") || stats.includes("tok/s"), "chat statistics were not rendered");

  const reasoningNode = page.locator("details.msg.reasoning").first();
  assert((await reasoningNode.count()) === 1, "reasoning block was not rendered");
  assert(!(await reasoningNode.evaluate((el) => el.open)), "reasoning should start collapsed");
  await reasoningNode.locator("summary").click();
  assert(await reasoningNode.evaluate((el) => el.open), "reasoning block did not expand");

  const chatSurface = await page.evaluate(() => {
    const log = document.querySelector("#chat-log");
    log.scrollTop = log.scrollHeight;
    const style = getComputedStyle(log);
    return {
      overflowY: style.overflowY,
      scrollTop: log.scrollTop,
      clientHeight: log.clientHeight,
      scrollHeight: log.scrollHeight,
    };
  });
  assert(
    chatSurface.overflowY === "auto" || chatSurface.overflowY === "scroll",
    `#chat-log must be the scroll host, got overflow-y=${chatSurface.overflowY}`,
  );
  assert(
    chatSurface.scrollTop + chatSurface.clientHeight >= chatSurface.scrollHeight - 48,
    `#chat-log did not settle at the history tail: ${JSON.stringify(chatSurface)}`,
  );

  const assistantBox = await page.locator("#chat-log .msg.assistant").first().evaluate((el) => {
    const s = getComputedStyle(el);
    const box = el.getBoundingClientRect();
    return {
      width: box.width,
      padding: parseFloat(s.paddingTop) + parseFloat(s.paddingLeft),
      background: s.backgroundColor,
      backgroundImage: s.backgroundImage,
    };
  });
  // The design renders assistant answers as plain text on the chat surface,
  // so width (not card padding) is what proves the body is laid out.
  assert(assistantBox.width > 200, "assistant message has no width");

  const codeBox = await page.locator("#chat-log .md-code").first().evaluate((el) => {
    const s = getComputedStyle(el);
    const pre = el.querySelector("pre");
    const ps = pre ? getComputedStyle(pre) : null;
    return {
      padding: parseFloat(s.paddingTop) + parseFloat(s.paddingLeft),
      prePadding: ps ? parseFloat(ps.paddingTop) + parseFloat(ps.paddingLeft) : 0,
      background: s.backgroundColor,
      backgroundImage: s.backgroundImage,
    };
  });
  assert(codeBox.padding > 0 || codeBox.prePadding > 0, "code block has no padding");
  assert(
    codeBox.background !== "rgba(0, 0, 0, 0)" || codeBox.backgroundImage !== "none",
    "code block background is transparent",
  );

  const codeColors = await page.evaluate(() => {
    const code = document.querySelector("#chat-log .md-code code.hljs");
    const keyword = document.querySelector("#chat-log .md-code code.hljs .hljs-keyword");
    return {
      code: code ? getComputedStyle(code).color : null,
      keyword: keyword ? getComputedStyle(keyword).color : null,
    };
  });
  assert(codeColors.code && codeColors.keyword, "highlighted code tokens were not found");
  assert(
    codeColors.keyword !== codeColors.code,
    `hljs keyword ${codeColors.keyword} must differ from code ${codeColors.code}`,
  );

  const reasoningOptions = await page.locator("#chat-reasoning option").count();
  assert(reasoningOptions === 4, `expected 4 reasoning options, got ${reasoningOptions}`);

  await page.evaluate(() => {
    if (document.querySelectorAll(".chat-item").length < 2) document.querySelector("#chat-new").click();
  });
  await page.waitForFunction(() => document.querySelectorAll(".chat-item").length >= 2);
  const chatItemStyles = await page.evaluate(() => {
    const active = document.querySelector(".chat-item.active");
    const inactive = [...document.querySelectorAll(".chat-item")].find(
      (el) => !el.classList.contains("active"),
    );
    if (!active || !inactive) return null;
    const a = getComputedStyle(active);
    const i = getComputedStyle(inactive);
    return {
      active: [a.backgroundColor, a.color, a.borderColor],
      inactive: [i.backgroundColor, i.color, i.borderColor],
    };
  });
  assert(chatItemStyles, "need active and inactive chat items to compare");
  assert(
    chatItemStyles.active.some((value, index) => value !== chatItemStyles.inactive[index]),
    `active chat item is not visually distinct: ${JSON.stringify(chatItemStyles)}`,
  );
  assert(await page.locator("#chat-reasoning-wrap").isVisible(), "reasoning selector is hidden");

  await page.setInputFiles("#chat-file-input", {
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("hello attachment"),
  });
  await page.waitForFunction(
    () => document.querySelector("#chat-attachments")?.textContent.includes("notes.txt"),
  );
  const attachmentChips = await page.locator("#chat-attachments .chat-attachment-chip").count();
  assert(attachmentChips === 1, `expected one attachment chip, got ${attachmentChips}`);
  await page.locator("[data-remove-attachment]").click();
  assert(await page.locator("#chat-attachments").isHidden(), "attachment chip was not removed");

  await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(["pasted"], "pasted.txt", { type: "text/plain" }));
    const event = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", { value: transfer });
    document.querySelector("#chat-text").dispatchEvent(event);
  });
  await page.waitForFunction(
    () => document.querySelector("#chat-attachments")?.textContent.includes("pasted.txt"),
  );
  await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(["dropped"], "dropped.txt", { type: "text/plain" }));
    const event = new Event("drop", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "dataTransfer", { value: transfer });
    document.querySelector(".chat-layout").dispatchEvent(event);
  });
  await page.waitForFunction(
    () => document.querySelector("#chat-attachments")?.textContent.includes("dropped.txt"),
  );
  await page.locator("[data-remove-attachment]").first().click();
  await page.locator("[data-remove-attachment]").first().click();
  assert(await page.locator("#chat-attachments").isHidden(), "paste/drop attachments were not removed");

  // history grouping by date
  assert(
    (await page.locator("#chat-list .hist-group").count()) >= 1,
    "chat history is not grouped by date",
  );
  const firstGroup = await page.locator("#chat-list .hist-group").first().textContent();
  assert(firstGroup.trim() === "Сегодня", `unexpected first history group: ${firstGroup}`);

  // rename through the history item (double click, Enter)
  const renameTarget = page.locator(".chat-item").first();
  const oldTitle = (await renameTarget.textContent()).trim();
  await renameTarget.dblclick();
  await page.waitForSelector(".chat-rename");
  await page.fill(".chat-rename", "Переименованный чат");
  await page.press(".chat-rename", "Enter");
  await page.waitForFunction(
    () => document.querySelector("#chat-list")?.textContent.includes("Переименованный чат"),
  );
  const renamed = await page.evaluate(
    async () => (await invoke("chat_list", { app_id: ctx.appId }))[0].title,
  );
  assert(renamed === "Переименованный чат", `rename did not persist: ${renamed} (was ${oldTitle})`);

  // custom confirmation instead of window.confirm
  let nativeDialog = false;
  page.on("dialog", (dialog) => {
    nativeDialog = true;
    dialog.dismiss();
  });
  await page.locator(".chat-del").first().click();
  await page.waitForSelector("#confirm-overlay:not([hidden])");
  const confirmTitle = (await page.locator("#confirm-title").textContent()).trim();
  assert(confirmTitle.length > 0, "confirm dialog has no title");
  await page.click("#confirm-cancel");
  await page.waitForFunction(
    () => document.getElementById("confirm-overlay")?.hidden === true,
  );
  assert(!nativeDialog, "a native window.confirm dialog was used");
  assert(
    (await page.locator(".chat-item").count()) > 0,
    "cancelled delete removed a chat",
  );

  // collapsed rail + Ctrl+K peek
  await page.click("#btn-collapse");
  await page.waitForFunction(() => document.querySelector(".app").classList.contains("collapsed"));
  assert(await page.locator("#btn-rail-hist").isVisible(), "history rail button is not visible");
  const railWidth = await page.evaluate(() => document.querySelector(".side").getBoundingClientRect().width);
  assert(railWidth < 80, `collapsed sidebar is ${railWidth}px wide`);
  await page.keyboard.press("Control+k");
  await page.waitForFunction(() => document.querySelector(".app").classList.contains("peek"));
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector(".app").classList.contains("peek"));
  await page.click("#btn-collapse");
  await page.waitForFunction(() => !document.querySelector(".app").classList.contains("collapsed"));

  // toast stack keeps several messages and dismisses on click
  await page.evaluate(() => {
    toast("первое уведомление");
    toast("второе уведомление", true);
  });
  await page.waitForFunction(() => document.querySelectorAll("#toast .toast").length >= 2);
  const toastCount = await page.locator("#toast .toast").count();
  assert(toastCount >= 2, `toast stack has ${toastCount} items`);
  await page.locator("#toast .toast").first().click();
  await page.waitForFunction(() => document.querySelectorAll("#toast .toast").length < 2);

  await page.evaluate(() => {
    ctx.pendingPermissions = [
      {
        id: "perm-smoke-1",
        app_id: ctx.appId,
        session_id: ctx.chatId,
        action: "platform_write_file",
        tool: "write_file",
        stream: "write",
        patterns: ["*"],
        always: [],
        metadata: { target: "about-elephants.md" },
        message_id: "msg_1",
        call_id: "call_1",
        created: 0,
        can_session: true,
        can_always: true,
        package_exec: false,
        bin: null,
        bin_sha256: null,
      },
    ];
    renderPermissionPrompt();
  });
  await page.waitForFunction(
    () => document.querySelector("#chat-permission")?.textContent.includes("about-elephants.md"),
    null,
    { timeout: 5000 },
  );
  assert(
    (await page.locator("#chat-permission .chat-permission-target code").textContent()) ===
      "about-elephants.md",
    "permission card did not show the target file name",
  );
  const permissionLabel = await page.locator("#chat-permission").textContent();
  assert(
    permissionLabel.includes("инструмент платформы"),
    `unexpected permission source label: ${permissionLabel}`,
  );
  assert(
    !permissionLabel.includes("инструмент пакета"),
    `contradictory package label on a platform tool: ${permissionLabel}`,
  );

  const permissionBox = await page.locator("#chat-permission").boundingBox();
  assert(permissionBox, "#chat-permission is not rendered");
  const viewport = page.viewportSize();
  assert(
    permissionBox.x >= -0.5 &&
      permissionBox.y >= -0.5 &&
      permissionBox.x + permissionBox.width <= viewport.width + 0.5 &&
      permissionBox.y + permissionBox.height <= viewport.height + 0.5,
    `permission card escapes the viewport: ${JSON.stringify(permissionBox)} for ${JSON.stringify(viewport)}`,
  );

  await page.evaluate(() => {
    ctx.pendingPermissions = [
      {
        id: "perm-smoke-2",
        app_id: ctx.appId,
        session_id: ctx.chatId,
        action: "platform_write_file",
        tool: "write_file",
        stream: "write",
        patterns: ["*"],
        always: [],
        metadata: {},
        message_id: "msg_1",
        call_id: "call_2",
        created: 0,
        can_session: true,
        can_always: true,
        package_exec: false,
        bin: null,
        bin_sha256: null,
      },
    ];
    renderPermissionPrompt();
  });
  await page.waitForFunction(
    () => {
      const box = document.querySelector("#chat-permission");
      return box && !box.hidden && box.textContent.includes("рабочая папка");
    },
    null,
    { timeout: 5000 },
  );
  assert(
    (await page.locator("#chat-permission .chat-permission-target").count()) === 0,
    "permission card must not render a target block without metadata.target",
  );
  await page.evaluate(() => {
    ctx.pendingPermissions = [];
    renderPermissionPrompt();
  });
  assert(
    await page.locator("#chat-permission").isHidden(),
    "permission card was not cleared after resolving the mock request",
  );

  await page.selectOption("#app-switch", "doc-lookup");
  await page.waitForFunction(
    () => document.querySelector("#chat-list")?.textContent.includes("Tauri v2"),
    null,
    { timeout: 5000 },
  );

  await page.click('.ws-tab[data-ws="settings"]');
  await page.waitForSelector("#ws-settings:not([hidden])");
  await page.waitForFunction(
    () => document.querySelector("#app-mcp-list")?.textContent.includes("context7"),
    null,
    { timeout: 5000 },
  );
  assert(
    (await page.locator("#app-perm-write").inputValue()) === "inherit",
    "per-app write policy was not initialized",
  );
  assert(
    (await page.locator("#app-perm-exec").inputValue()) === "inherit",
    "per-app exec policy was not initialized",
  );

  await page.selectOption("#app-perm-write", "inherit");
  await page.selectOption("#app-perm-exec", "deny");
  await page.click("#app-perm-save");
  await page.waitForFunction(
    () => document.querySelector("#toast")?.textContent.includes("Правила подтверждений сохранены"),
    null,
    { timeout: 5000 },
  );
  const savedPolicy = await page.evaluate(
    async () => (await invoke("get_config")).permissions.apps["doc-lookup"],
  );
  assert(
    savedPolicy && savedPolicy.exec === "deny",
    `expected the exec override to be saved, got ${JSON.stringify(savedPolicy)}`,
  );
  assert(
    savedPolicy && !("write" in savedPolicy),
    `an inherited field must not be materialised: ${JSON.stringify(savedPolicy)}`,
  );

  await page.click('.tab[data-tab="settings"]');
  await page.waitForSelector("#tab-settings.active");
  assert(
    (await page.locator("#perm-write").inputValue()) === "ask",
    "global write policy was not initialized",
  );
  assert(
    (await page.locator("#perm-exec").inputValue()) === "ask",
    "global exec policy was not initialized",
  );
  const artifactUsage = await page.locator("#artifact-usage").textContent();
  assert(artifactUsage.includes("2 сессий"), `unexpected artifact usage: ${artifactUsage}`);
  const dumpRows = await page.locator("#artifact-dumps .artifact-row").count();
  assert(dumpRows === 2, `expected 2 mock dumps, got ${dumpRows}`);

  await page.fill("#artifact-query", "mock");
  await page.click("#artifact-search");
  await page.waitForFunction(
    () => document.querySelectorAll("#artifact-results .artifact-row").length > 0,
  );
  const hitText = await page.locator("#artifact-results").textContent();
  assert(hitText.includes("mock hit"), `artifact search did not render hits: ${hitText}`);

  await page.click('.tab[data-tab="dev"]');
  await page.waitForSelector("#tab-dev.active");
  const packStatus = await page.locator("#pack-status").textContent();
  assert(packStatus.includes("eco-pack"), `unexpected eco-pack status: ${packStatus}`);
  await page.fill("#pack-dir", "D:\\apps\\demo-app");
  await page.click("#pack-load");
  await page.waitForFunction(
    () => document.querySelectorAll("#pack-files .pack-file").length >= 2,
  );
  const manifestText = await page.locator("#pack-editor").inputValue();
  assert(manifestText.includes("demo-app"), "manifest editor was not loaded");
  const scanLog = await page.locator("#pack-log").textContent();
  assert(scanLog.includes("AGENTS.md: найден"), `scan summary missing: ${scanLog}`);
  await page.waitForFunction(
    () => document.querySelector("#pack-mcp")?.textContent.includes("context7"),
  );
  const mcpText = await page.locator("#pack-mcp").textContent();
  assert(mcpText.includes("сервер настроен"), `unexpected MCP status: ${mcpText}`);
  await page.locator('[data-mcp-write="context7"]').click();
  await page.waitForFunction(
    () => document.querySelector("#pack-log")?.textContent.includes("запись разрешена"),
  );
  await page.click("#pack-import");
  await page.waitForFunction(
    () => document.querySelector("#pack-log")?.textContent.includes("manifest.json created"),
  );
  await page.click("#pack-apply-template");
  await page.waitForFunction(
    () => document.querySelector("#pack-log")?.textContent.includes("file_scopes"),
  );
  await page.click("#pack-validate");
  await page.waitForFunction(
    () => document.querySelector("#pack-log")?.textContent.includes("eco-pack"),
  );

  await page.click('.tab[data-tab="settings"]');
  await page.click("#mode-change");
  await page.waitForSelector("#mode-overlay:not([hidden])");
  const modeLayout = await page.evaluate(() => {
    const dialog = document.querySelector(".mode-dialog");
    const radio = document.querySelector(".mode-radio");
    return {
      overflow: dialog.scrollWidth - dialog.clientWidth,
      cards: document.querySelectorAll(".mode-card").length,
      radioDirection: radio ? getComputedStyle(radio).flexDirection : "",
    };
  });
  assert(modeLayout.overflow <= 1, `mode dialog overflows horizontally by ${modeLayout.overflow}px`);
  assert(modeLayout.cards === 2, "mode dialog must show both mode cards");
  assert(modeLayout.radioDirection === "row", "mode radio labels must be horizontal");
  await page.check('input[name="mode-role"][value="user"]');
  await page.click('[data-mode="local"]');
  await page.waitForFunction(
    () => document.querySelector("#mode-badge")?.textContent.includes("локальный"),
  );
  assert(
    (await page.locator("#mode-setting").textContent()).includes("пользователь"),
    "mode setting was not updated",
  );
  assert(await page.locator('.tab[data-tab="dev"]').isHidden(), "developer tab must be hidden for a user");
  assert(
    (await page.locator("#allowlist").textContent()).includes("не требуется"),
    "local mode must not show the allowlist gate",
  );

  async function layoutSnapshot(width, height, theme) {
    await page.setViewportSize({ width, height });
    await page.evaluate((nextTheme) => {
      try {
        localStorage.setItem("theme", nextTheme);
      } catch (_) {}
    }, theme);
    await page.reload();
    await page.waitForSelector("#apps-panel .card");

    const catalog = await page.evaluate(() => {
      const hasScrollAncestor = (el) => {
        for (let node = el.parentElement; node && node !== document.body; node = node.parentElement) {
          const overflowX = getComputedStyle(node).overflowX;
          if (overflowX === "auto" || overflowX === "scroll" || overflowX === "hidden") return true;
        }
        return false;
      };
      const offenders = [...document.querySelectorAll("body *")]
        .filter((el) => {
          const style = getComputedStyle(el);
          if (style.display === "none" || style.visibility === "hidden" || style.position === "fixed") return false;
          const rect = el.getBoundingClientRect();
          return rect.width > window.innerWidth + 1 && !hasScrollAncestor(el);
        })
        .slice(0, 4)
        .map((el) => `${el.tagName.toLowerCase()}.${String(el.className).split(/\s+/).join(".")}`);
      const clipped = (el) => {
        const s = getComputedStyle(el);
        return s.textOverflow === "ellipsis" || (s.webkitLineClamp && s.webkitLineClamp !== "none");
      };
      // Ellipsized/clamped text intentionally reports scrollWidth > clientWidth.
      const overflowItems = (root) => {
        if (!root) return [];
        return [...root.querySelectorAll("*")]
          .filter((el) => {
            const s = getComputedStyle(el);
            if (s.display === "none" || s.visibility === "hidden") return false;
            const r = el.getBoundingClientRect();
            if (r.width <= 0 || r.height <= 0) return false;
            return el.scrollWidth > el.clientWidth + 1 && !clipped(el);
          })
          .slice(0, 4)
          .map((el) => `${el.tagName.toLowerCase()}#${el.id}.${String(el.className).split(/\s+/).join(".")}`);
      };
      const grid = document.querySelector("#apps");
      const firstCard = document.querySelector("#apps .card");
      const firstCardHead = firstCard ? firstCard.querySelector(".app-card-top") : null;
      const topbar = document.querySelector(".topbar");
      return {
        theme: document.documentElement.dataset.theme,
        innerWidth: window.innerWidth,
        innerHeight: window.innerHeight,
        bodyScrollWidth: document.body.scrollWidth,
        docScrollWidth: document.documentElement.scrollWidth,
        columns: getComputedStyle(grid).gridTemplateColumns.split(" ").filter(Boolean).length,
        cards: document.querySelectorAll("#apps .card").length,
        firstCardTop: firstCard ? Math.round(firstCard.getBoundingClientRect().top) : -1,
        firstCardHeadBottom: firstCardHead ? Math.round(firstCardHead.getBoundingClientRect().bottom) : -1,
        topbarOverflow: overflowItems(topbar),
        offenders,
      };
    });
    assert(catalog.theme === theme, `theme was not applied for ${width}x${height}: ${catalog.theme}`);
    assert(
      catalog.docScrollWidth <= catalog.innerWidth + 1 &&
        catalog.bodyScrollWidth <= catalog.innerWidth + 1,
      `catalog horizontal overflow at ${width}x${height}: ${JSON.stringify(catalog)}`,
    );
    assert(
      catalog.offenders.length === 0,
      `catalog elements wider than viewport at ${width}x${height}: ${catalog.offenders.join(", ")}`,
    );
    if (width <= 900) {
      assert(
        catalog.columns === 2,
        `catalog must show 2 columns at ${width}px, got ${catalog.columns}`,
      );
    }
    assert(
      catalog.topbarOverflow.length === 0,
      `topbar has clipped horizontal content at ${width}x${height}: ${catalog.topbarOverflow.join(", ")}`,
    );
    if (width === 900 && height === 600) {
      assert(
        catalog.firstCardTop > 0 && catalog.firstCardTop < catalog.innerHeight,
        `first app card starts below the fold at 900x600: top=${catalog.firstCardTop}`,
      );
      assert(
        catalog.firstCardHeadBottom > 0 && catalog.firstCardHeadBottom <= catalog.innerHeight + 1,
        `first app card title/badges are clipped at 900x600: bottom=${catalog.firstCardHeadBottom}`,
      );
    }
    await page.screenshot({ path: join(SHOT_DIR, `catalog-${theme}-${width}x${height}.png`) });

    await page.locator('[data-open="doc-writer"]').click();
    await page.waitForSelector("#workspace:not([hidden])");
    await page.waitForFunction(
      () => document.querySelector("#chat-log")?.textContent.includes("report.docx"),
      null,
      { timeout: 10000 },
    );

    const chat = await page.evaluate(() => {
      const read = (selector) => {
        const el = document.querySelector(selector);
        if (!el) return null;
        const style = getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return {
          hidden: Boolean(el.hidden),
          display: style.display,
          visible:
            !el.hidden &&
            style.display !== "none" &&
            style.visibility !== "hidden" &&
            rect.width > 0 &&
            rect.height > 0,
          left: rect.left,
          top: rect.top,
          right: rect.right,
          bottom: rect.bottom,
          width: rect.width,
          height: rect.height,
        };
      };
      const hasScrollAncestor = (el) => {
        for (let node = el.parentElement; node && node !== document.body; node = node.parentElement) {
          const overflowX = getComputedStyle(node).overflowX;
          if (overflowX === "auto" || overflowX === "scroll" || overflowX === "hidden") return true;
        }
        return false;
      };
      const offenders = [...document.querySelectorAll("body *")]
        .filter((el) => {
          const style = getComputedStyle(el);
          if (style.display === "none" || style.visibility === "hidden" || style.position === "fixed") return false;
          const rect = el.getBoundingClientRect();
          return rect.width > window.innerWidth + 1 && !hasScrollAncestor(el);
        })
        .slice(0, 4)
        .map((el) => `${el.tagName.toLowerCase()}.${String(el.className).split(/\s+/).join(".")}`);
      const clipped = (el) => {
        const s = getComputedStyle(el);
        return s.textOverflow === "ellipsis" || (s.webkitLineClamp && s.webkitLineClamp !== "none");
      };
      // Ellipsized/clamped text intentionally reports scrollWidth > clientWidth.
      const overflowItems = (root) => {
        if (!root) return [];
        return [...root.querySelectorAll("*")]
          .filter((el) => {
            const s = getComputedStyle(el);
            if (s.display === "none" || s.visibility === "hidden") return false;
            const r = el.getBoundingClientRect();
            if (r.width <= 0 || r.height <= 0) return false;
            return el.scrollWidth > el.clientWidth + 1 && !clipped(el);
          })
          .slice(0, 4)
          .map((el) => `${el.tagName.toLowerCase()}#${el.id}.${String(el.className).split(/\s+/).join(".")}`);
      };
      const log = document.querySelector("#chat-log");
      log.scrollTop = log.scrollHeight;
      const logStyle = getComputedStyle(log);
      return {
        innerWidth: window.innerWidth,
        innerHeight: window.innerHeight,
        docScrollWidth: document.documentElement.scrollWidth,
        bodyScrollWidth: document.body.scrollWidth,
        workspace: read("#workspace"),
        topbar: read(".topbar"),
        head: read(".c-row"),
        log: read("#chat-log"),
        composer: read(".chat-composer-wrap"),
        topbarOverflow: overflowItems(document.querySelector(".topbar")),
        headOverflow: overflowItems(document.querySelector(".c-row")),
        logOverflowY: logStyle.overflowY,
        logScrollTop: log.scrollTop,
        logScrollHeight: log.scrollHeight,
        logClientHeight: log.clientHeight,
        offenders,
      };
    });
    for (const [name, box] of [
      ["#workspace", chat.workspace],
      ["#chat-log", chat.log],
      [".chat-composer-wrap", chat.composer],
    ]) {
      assert(
        box && box.visible,
        `${name} is not visible at ${width}x${height}: ${JSON.stringify(box)}`,
      );
      assert(
        box.left >= -1 &&
          box.top >= -1 &&
          box.right <= chat.innerWidth + 1 &&
          box.bottom <= chat.innerHeight + 1,
        `${name} escapes the viewport at ${width}x${height}: ${JSON.stringify(box)}`,
      );
    }
    assert(
      chat.logOverflowY === "auto" || chat.logOverflowY === "scroll",
      `#chat-log is not the scroll host at ${width}x${height}: ${chat.logOverflowY}`,
    );
    assert(
      chat.logScrollTop + chat.logClientHeight >= chat.logScrollHeight - 48,
      `#chat-log history is not at the tail at ${width}x${height}: ${JSON.stringify(chat)}`,
    );
    assert(
      chat.docScrollWidth <= chat.innerWidth + 1 &&
        chat.bodyScrollWidth <= chat.innerWidth + 1,
      `chat horizontal overflow at ${width}x${height}: ${JSON.stringify(chat)}`,
    );
    assert(
      chat.offenders.length === 0,
      `chat elements wider than viewport at ${width}x${height}: ${chat.offenders.join(", ")}`,
    );
    assert(
      chat.topbar && chat.topbar.height > 40 && chat.topbar.height <= 64,
      `topbar is ${chat.topbar && chat.topbar.height}px tall at ${width}x${height} (expected 40..64)`,
    );
    assert(
      chat.topbarOverflow.length === 0,
      `workspace topbar content overflows at ${width}x${height}: ${chat.topbarOverflow.join(", ")}`,
    );
    assert(
      chat.headOverflow.length === 0,
      `chat head content overflows at ${width}x${height}: ${chat.headOverflow.join(", ")}`,
    );
    if (width === 900 && height === 600) {
      assert(
        chat.log.height >= 240,
        `#chat-log is only ${chat.log.height}px tall at 900x600 (min 240)`,
      );
    }
    await page.screenshot({ path: join(SHOT_DIR, `chat-${theme}-${width}x${height}.png`) });
    return { catalog, chat };
  }

  const hasChassis = await page.evaluate(() => Boolean(document.querySelector(".app > .side")));
  if (!hasChassis) {
    console.log("layout chassis not present: skipping Wave 1+2 layout checks");
  }
  const layoutReports = [];
  if (hasChassis) {
  for (const [width, height, theme] of [
    [1180, 900, "dark"],
    [1180, 900, "light"],
    [900, 600, "dark"],
    [900, 600, "light"],
  ]) {
    layoutReports.push(await layoutSnapshot(width, height, theme));
  }
  for (const report of layoutReports) {
    console.log(
      `layout ${report.catalog.innerWidth}x${report.chat.innerHeight}: theme=${report.catalog.theme} cols=${report.catalog.columns} workspace=${Math.round(report.chat.workspace.width)}x${Math.round(report.chat.workspace.height)} topbar=${Math.round(report.chat.topbar.height)} log=${Math.round(report.chat.log.width)}x${Math.round(report.chat.log.height)} scroll=${report.chat.logClientHeight}/${report.chat.logScrollHeight} firstCardTop=${report.catalog.firstCardTop} composerBottom=${Math.round(report.chat.composer.bottom)}`,
    );
  }

  // Ending the session must clear the app-bound history: a stale item would
  // call chat_select without an app id and surface an IPC error.
  assert(await page.locator("#workspace").isVisible(), "expected an open session before closing");
  await page.click("#ws-close");
  await page.waitForFunction(() => document.querySelector("#chat-list")?.children.length === 0);
  assert(
    (await page.locator("#chat-list .chat-item").count()) === 0,
    "history survived closing the session",
  );
  assert(await page.locator("#apps-panel").isVisible(), "catalog is not shown after closing");
  assert(
    (await page.locator("#toast .toast.error").count()) === 0,
    "closing the session produced an error toast",
  );
  assert(
    (await page.locator("#ws-title").textContent()) === "Агент не выбран",
    "agent card was not reset after closing the session",
  );

  // Settings and dev tabs must lay out without horizontal clipping at 900x600.
  await page.setViewportSize({ width: 900, height: 600 });
  if (await page.locator("#workspace").isVisible()) {
    await page.click("#ws-back");
    await page.waitForSelector("#apps-panel:not([hidden])");
  }
  await page.click('.tab[data-tab="settings"]');
  await page.waitForSelector("#tab-settings.active");
  await page.waitForFunction(
    () => (document.querySelector("#llm-endpoint")?.value || "").length > 0,
  );
  assert(await page.locator("#sheet-scrim").isVisible(), "settings sheet has no scrim");
  assert(await page.locator("#settings-close").isVisible(), "settings sheet has no close button");
  await page.waitForTimeout(300);
  const sheetBox = await page.locator("#tab-settings").boundingBox();
  assert(
    sheetBox && sheetBox.x > 0 && sheetBox.x + sheetBox.width <= 901,
    `settings sheet is outside the viewport: ${JSON.stringify(sheetBox)}`,
  );
  await page.click("#settings-close");
  await page.waitForSelector("#tab-apps.active");
  assert(await page.locator("#sheet-scrim").isHidden(), "scrim stayed after closing the sheet");
  await page.click('.tab[data-tab="settings"]');
  await page.waitForSelector("#tab-settings.active");

  const tabOverflow = async (selector, controlIds = []) =>
    page.evaluate(
      ({ selector, controlIds }) => {
        const tab = document.querySelector(selector);
        if (!tab) return null;
        const b = tab.getBoundingClientRect();
        const clipped = (el) => {
          const s = getComputedStyle(el);
          return s.textOverflow === "ellipsis" || (s.webkitLineClamp && s.webkitLineClamp !== "none");
        };
        const offenders = [...tab.querySelectorAll("*")]
          .filter((el) => {
            const s = getComputedStyle(el);
            if (s.display === "none" || s.visibility === "hidden") return false;
            const r = el.getBoundingClientRect();
            if (r.width <= 0 || r.height <= 0) return false;
            return r.right > window.innerWidth + 1 || r.left < -1;
          })
          .slice(0, 6)
          .map((el) => `${el.tagName.toLowerCase()}#${el.id}.${String(el.className).split(/\s+/).join(".")}`);
        const scrollOffenders = [...tab.querySelectorAll("*")]
          .filter((el) => {
            const s = getComputedStyle(el);
            if (s.display === "none" || s.visibility === "hidden") return false;
            const r = el.getBoundingClientRect();
            if (r.width <= 0 || r.height <= 0) return false;
            return el.scrollWidth > el.clientWidth + 1 && !clipped(el);
          })
          .slice(0, 6)
          .map((el) => `${el.tagName.toLowerCase()}#${el.id}.${String(el.className).split(/\s+/).join(".")}`);
        const controls = controlIds.map((id) => {
          const el = document.querySelector(id);
          if (!el) return { id, visible: false };
          const r = el.getBoundingClientRect();
          const s = getComputedStyle(el);
          return {
            id,
            visible: r.width > 0 && r.height > 0 && s.display !== "none" && s.visibility !== "hidden",
          };
        });
        return {
          box: { left: Math.round(b.left), right: Math.round(b.right), width: Math.round(b.width) },
          scrollWidth: tab.scrollWidth,
          clientWidth: tab.clientWidth,
          offenders,
          scrollOffenders,
          controls,
        };
      },
      { selector, controlIds },
    );

  const settingsReports = [];
  for (const theme of ["light", "dark"]) {
    await page.evaluate((nextTheme) => {
      document.documentElement.dataset.theme = nextTheme;
      try {
        localStorage.setItem("theme", nextTheme);
      } catch (_) {}
    }, theme);
    await page.waitForTimeout(260);
    const report = await tabOverflow("#tab-settings", [
      "#perm-write",
      "#llm-endpoint",
      "#artifact-query",
      "#mcp-servers",
    ]);
    assert(report, "#tab-settings was not found");
    assert(
      report.scrollWidth <= report.clientWidth + 1 && report.offenders.length === 0 && report.scrollOffenders.length === 0,
      `settings tab overflows at 900x600 (${theme}): ${JSON.stringify(report)}`,
    );
    for (const control of report.controls) {
      assert(control.visible, `${control.id} is not visible in the settings tab at 900x600`);
    }
    await page.screenshot({ path: join(SHOT_DIR, `settings-${theme}-900x600.png`) });
    settingsReports.push({ theme, ...report });
  }

  await page.click('.tab[data-tab="dev"]');
  await page.waitForSelector("#tab-dev.active");
  const devReports = [];
  for (const theme of ["light", "dark"]) {
    await page.evaluate((nextTheme) => {
      document.documentElement.dataset.theme = nextTheme;
      try {
        localStorage.setItem("theme", nextTheme);
      } catch (_) {}
    }, theme);
    await page.waitForTimeout(260);
    const report = await tabOverflow("#tab-dev");
    assert(report, "#tab-dev was not found");
    assert(
      report.scrollWidth <= report.clientWidth + 1 && report.offenders.length === 0 && report.scrollOffenders.length === 0,
      `dev tab overflows at 900x600 (${theme}): ${JSON.stringify(report)}`,
    );
    await page.screenshot({ path: join(SHOT_DIR, `dev-${theme}-900x600.png`) });
    devReports.push({ theme, ...report });
  }
  console.log(
    `settings/dev 900x600: settings=${settingsReports.map((r) => `${r.theme}:${r.clientWidth}/${r.scrollWidth}`).join(",")} dev=${devReports.map((r) => `${r.theme}:${r.clientWidth}/${r.scrollWidth}`).join(",")} controls=${settingsReports[0].controls.filter((c) => c.visible).length}/${settingsReports[0].controls.length}`,
  );
  }
  assert(
    externalRequests.length === 0,
    `external requests from the UI: ${externalRequests.join(", ")}`,
  );
  assert(pageErrors.length === 0, `page errors: ${pageErrors.join("; ")}`);
  console.log(
    "ui smoke ok: catalog, allowlist, chat, highlight, workspace files, diagrams, app settings, artifacts, attachments, permission target, package import, modes, topbar density, chat-log height, settings/dev overflow, history groups, rename, confirm dialog, collapsed rail, toast stack, session close",
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
