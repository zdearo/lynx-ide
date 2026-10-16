/**
 * Drives the interface in headless Chrome against the in-memory backend
 * (`?mock=`), so no desktop window is involved. Run with `pnpm e2e`.
 * Set CHROME_PATH when Chrome is not at a standard location.
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { chromium } from "playwright-core";

const PORT = 1431;
const BASE = `http://localhost:${PORT}/`;
const require = createRequire(import.meta.url);
const axeSource = readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");

const chromePath = [
  process.env.CHROME_PATH,
  "/usr/bin/google-chrome-stable",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
].find((candidate) => candidate && existsSync(candidate));
if (!chromePath) {
  console.error("Chrome not found. Install it or set CHROME_PATH.");
  process.exit(2);
}

const vite = spawn(
  "pnpm",
  ["exec", "vite", "--port", String(PORT), "--strictPort"],
  {
    stdio: "ignore",
  },
);
for (let attempt = 0; ; attempt++) {
  try {
    if ((await fetch(BASE)).ok) break;
  } catch {
    // Not up yet.
  }
  if (attempt > 60) {
    vite.kill();
    console.error("The dev server did not start.");
    process.exit(2);
  }
  await new Promise((resolve) => setTimeout(resolve, 250));
}

const browser = await chromium.launch({
  executablePath: chromePath,
  headless: true,
  args: ["--no-sandbox"],
});
const results = [];
const check = (name, ok, detail = "") =>
  results.push(`${ok ? "PASS" : "FAIL"}  ${name}${ok ? "" : "  -> " + detail}`);

async function session(query, width, height, run) {
  const context = await browser.newContext({ viewport: { width, height } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(BASE + query, { waitUntil: "networkidle" });
  await page.waitForTimeout(800);
  await run(page);
  check(
    `no console errors (${query} @${width})`,
    errors.length === 0,
    errors.join(" | "),
  );
  await context.close();
}
const tabs = (page) => page.getByRole("tab").count();
const overflow = (page) =>
  page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  check("selecting a worktree shows its 2 tabs", (await tabs(p)) === 2);

  await p.keyboard.press("Control+Shift+T");
  await p.waitForTimeout(300);
  check(
    "Ctrl+Shift+T opens a third tab",
    (await tabs(p)) === 3,
    String(await tabs(p)),
  );

  await p.keyboard.press("Control+Shift+W");
  await p.waitForTimeout(300);
  check(
    "Ctrl+Shift+W closes the active tab",
    (await tabs(p)) === 2,
    String(await tabs(p)),
  );

  await p.keyboard.press("Control+Shift+B");
  await p.waitForTimeout(200);
  check(
    "Ctrl+Shift+B hides the sidebar",
    (await p.locator("aside").count()) === 0,
  );
  check(
    "hidden sidebar shows the location in the tab bar",
    (await p.getByText("agent-ade / feat/auth").count()) === 1,
  );
  await p.keyboard.press("Control+Shift+B");

  const row = p.getByRole("button", { name: /^fix\/pty/ });
  await row.focus();
  await p.keyboard.press("ArrowDown");
  const focused = await p.evaluate(
    () => document.activeElement?.textContent ?? "",
  );
  check(
    "ArrowDown moves focus to the next row",
    focused.includes("api-server"),
    focused,
  );

  check(
    "no horizontal overflow",
    (await overflow(p)) <= 0,
    String(await overflow(p)),
  );

  await p.keyboard.press("Control+Shift+N");
  await p.locator("#worktree-name").fill("feature/payments");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(250);
  check(
    "dialog closes immediately and a progress row appears",
    (await p.getByText("Creating feature/payments…").count()) === 1,
  );
  await p.waitForTimeout(1500);
  const created = p.getByRole("button", { name: /^feature\/payments/ });
  check(
    "created worktree appears and becomes selected",
    (await created.count()) === 1 &&
      (await created.getAttribute("aria-current")) === "true",
  );

  await p.getByRole("button", { name: /^fix\/pty/ }).hover();
  await p.getByRole("button", { name: "Actions for fix/pty" }).click();
  await p.getByRole("menuitem", { name: /Remove worktree/ }).click();
  await p.getByRole("button", { name: "Remove", exact: true }).click();
  await p.waitForTimeout(400);
  check(
    "removed worktree disappears from the sidebar",
    (await p.getByRole("button", { name: /^fix\/pty/ }).count()) === 0,
  );

  await p.keyboard.press("Control+Shift+N");
  await p.locator("#worktree-name").fill("bad name");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(1900);
  check(
    "a refused name reopens the dialog with the reason and the typed name",
    (await p.locator("#worktree-name").inputValue()) === "bad name" &&
      (await p.getByRole("alert").count()) === 1,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the dialog open",
    axe.length === 0,
    axe.join(" || "),
  );
});

await session("?mock=full", 1200, 760, async (p) => {
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the main screen",
    axe.length === 0,
    axe.join(" || "),
  );
});

await session("?mock=empty", 1000, 700, async (p) => {
  await p.getByRole("button", { name: "Open repository" }).last().click();
  await p.waitForTimeout(500);
  check(
    "opening a repository adds it to the sidebar",
    (await p.getByText("web-app").count()) >= 1,
  );
  check(
    "its main worktree is selected and offers the launcher",
    (await p.getByText("Nothing is running in main").count()) === 1,
  );
});

await session("?mock=full", 800, 700, async (p) => {
  check(
    "narrow window starts with the sidebar closed",
    (await p.locator("aside").count()) === 0 &&
      (await p.getByText("Repositories").count()) === 0,
  );
  check("narrow window has no horizontal overflow", (await overflow(p)) <= 0);
  await p.keyboard.press("Control+Shift+B");
  await p.waitForTimeout(400);
  check(
    "Ctrl+Shift+B opens the drawer",
    (await p.getByText("Repositories").count()) >= 1,
  );
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  check(
    "choosing a worktree closes the drawer",
    (await p.getByText("Repositories").count()) === 0,
  );
  check(
    "the tab bar names the worktree when the sidebar is closed",
    (await p.getByText("agent-ade / feat/auth").count()) === 1,
  );
});

console.log(results.join("\n"));
await browser.close();
vite.kill();
process.exit(results.some((line) => line.startsWith("FAIL")) ? 1 : 0);
