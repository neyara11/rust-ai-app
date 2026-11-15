import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, isAbsolute, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const ROOT = fileURLToPath(new URL("../../src/", import.meta.url));
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

  const reasoningOptions = await page.locator("#chat-reasoning option").count();
  assert(reasoningOptions === 4, `expected 4 reasoning options, got ${reasoningOptions}`);
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

  await page.click('.tab[data-tab="settings"]');
  await page.waitForSelector("#tab-settings.active");
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

  assert(
    externalRequests.length === 0,
    `external requests from the UI: ${externalRequests.join(", ")}`,
  );
  assert(pageErrors.length === 0, `page errors: ${pageErrors.join("; ")}`);
  console.log(
    "ui smoke ok: catalog, allowlist, chat, diagrams, app settings, artifacts, attachments, package import, modes",
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
