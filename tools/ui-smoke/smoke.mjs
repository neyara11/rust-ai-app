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
page.on("pageerror", (error) => pageErrors.push(error));

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

  const stats = await page.locator("#chat-log").textContent();
  assert(stats.includes("58.4") || stats.includes("tok/s"), "chat statistics were not rendered");

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

  await page.selectOption("#app-switch", "doc-lookup");
  await page.waitForFunction(
    () => document.querySelector("#chat-list")?.textContent.includes("Tauri v2"),
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

  assert(pageErrors.length === 0, `page errors: ${pageErrors.join("; ")}`);
  console.log(
    "ui smoke ok: catalog, allowlist badge, trust gating, chat render, app switch, artifact search",
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
