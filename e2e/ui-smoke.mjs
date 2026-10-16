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

async function session(query, width, height, run, contextOptions = {}) {
  const context = await browser.newContext({
    viewport: { width, height },
    ...contextOptions,
  });
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
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.getByRole("tab", { name: /claude/ }).click();
  await p.waitForTimeout(800);

  // Room is what is left inside every ancestor once its own padding is removed.
  const fit = await p.evaluate(() => {
    const xterm = document.querySelector(".xterm");
    const screen = document.querySelector(".xterm-screen");
    if (!xterm || !screen) return null;
    const s = screen.getBoundingClientRect();
    let minBottom = Infinity;
    let minRight = Infinity;
    for (
      let el = xterm.parentElement;
      el && el.tagName !== "MAIN";
      el = el.parentElement
    ) {
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      minBottom = Math.min(
        minBottom,
        r.bottom -
          parseFloat(cs.paddingBottom) -
          parseFloat(cs.borderBottomWidth),
      );
      minRight = Math.min(
        minRight,
        r.right - parseFloat(cs.paddingRight) - parseFloat(cs.borderRightWidth),
      );
    }
    return {
      bottomGap: minBottom - s.bottom,
      rightGap: minRight - s.right,
      leftover: minBottom - s.bottom,
    };
  });
  check(
    "terminal rows fit: nothing is cut off at the bottom",
    fit !== null && fit.bottomGap >= -0.5,
    JSON.stringify(fit),
  );
  check(
    "terminal columns fit: nothing is cut off at the right",
    fit !== null && fit.rightGap >= -0.5,
    JSON.stringify(fit),
  );
  check(
    "terminal wastes less than one row of space",
    fit !== null && fit.leftover < 20,
    JSON.stringify(fit),
  );

  const lastRowVisible = await p.evaluate(() => {
    const rows = [...document.querySelectorAll(".xterm-rows > div")].filter(
      (r) => r.textContent?.trim(),
    );
    const last = rows.at(-1);
    const holder = document.querySelector(".xterm")?.parentElement;
    if (!last || !holder) return false;
    return (
      last.getBoundingClientRect().bottom <=
      holder.getBoundingClientRect().bottom + 0.5
    );
  });
  check("the last line of output is fully inside the terminal", lastRowVisible);

  const fonts = await p.evaluate(async () => {
    await document.fonts.ready;
    const loaded = [...document.fonts]
      .filter((f) => f.status === "loaded")
      .map((f) => f.family.replaceAll('"', ""));
    return { text: document.fonts.check('13px "JetBrains Mono"'), loaded };
  });
  check(
    "the bundled text font is loaded",
    fonts.text && fonts.loaded.some((f) => f.includes("JetBrains Mono")),
    JSON.stringify(fonts),
  );

  await p.getByRole("tab", { name: /zsh/ }).click();
  await p.waitForTimeout(1200);
  const symbols = await p.evaluate(() =>
    [...document.fonts].some(
      (f) =>
        f.family.includes("Symbols Nerd Font Mono") && f.status === "loaded",
    ),
  );
  check("the Nerd Font symbols load when an icon appears", symbols);
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

await session("?mock=full", 1200, 760, async (p) => {
  const nav = p.getByRole("navigation", { name: "Primary navigation" });
  const projects = nav.getByRole("button", { name: "Projects" });
  const settings = nav.getByRole("button", { name: "Settings" });
  const panel = p.getByRole("complementary", { name: "Projects" });

  check(
    "the primary bar shows Projects as pressed with its panel open",
    (await projects.getAttribute("aria-pressed")) === "true" &&
      (await panel.count()) === 1,
  );

  await projects.click();
  await p.waitForTimeout(200);
  check(
    "clicking the active view hides the side panel and keeps the primary bar",
    (await panel.count()) === 0 &&
      (await nav.count()) === 1 &&
      (await projects.getAttribute("aria-pressed")) === "false",
  );

  await projects.click();
  await p.waitForTimeout(200);
  check("clicking it again brings the panel back", (await panel.count()) === 1);

  await p.keyboard.press("Control+Shift+B");
  await p.waitForTimeout(200);
  check(
    "the sidebar shortcut hides the panel but not the primary bar",
    (await panel.count()) === 0 && (await nav.count()) === 1,
  );
  await p.keyboard.press("Control+Shift+B");

  const barBox = await nav.boundingBox();
  check(
    "the primary bar is a narrow strip at the far left",
    barBox !== null && barBox.x === 0 && barBox.width < 56,
    JSON.stringify(barBox),
  );
  const settingsBox = await settings.boundingBox();
  const projectsBox = await projects.boundingBox();
  check(
    "settings sits at the bottom of the bar, below the views",
    settingsBox !== null &&
      projectsBox !== null &&
      settingsBox.y > projectsBox.y + 300,
    `${settingsBox?.y} vs ${projectsBox?.y}`,
  );

  await settings.click();
  await p.waitForTimeout(400);
  const dialog = p.getByRole("dialog", { name: "Settings" });
  const box = await dialog.boundingBox();
  check(
    "settings opens as a large centered modal with margin around it",
    box !== null &&
      box.width >= 800 &&
      box.width <= 960 &&
      box.height >= 600 &&
      box.height <= 704 &&
      box.x >= 32 &&
      box.y >= 32 &&
      Math.abs(box.x + box.width / 2 - 600) < 2 &&
      Math.abs(box.y + box.height / 2 - 380) < 2,
    JSON.stringify(box),
  );
  const axeSettings = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations with settings open",
    axeSettings.length === 0,
    axeSettings.join(" || "),
  );
  await p.keyboard.press("Escape");
  await p.waitForTimeout(300);
  check(
    "Escape closes settings",
    (await p.getByRole("dialog", { name: "Settings" }).count()) === 0,
  );

  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  check(
    "Ctrl+, opens settings",
    (await p.getByRole("dialog", { name: "Settings" }).count()) === 1,
  );
  await p.getByRole("button", { name: "Close settings" }).click();
  await p.waitForTimeout(300);
  check(
    "the close button closes settings",
    (await p.getByRole("dialog", { name: "Settings" }).count()) === 0,
  );
});

const termFont = (p) =>
  p.evaluate(() => {
    const rows = document.querySelector(".xterm-rows");
    const cs = rows && getComputedStyle(rows);
    return cs ? { size: cs.fontSize, family: cs.fontFamily } : null;
  });

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(600);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(400);

  const size = p.locator("#terminal-font-size");
  check(
    "settings lists the sections and starts on Terminal",
    (await p.getByRole("tab", { name: "Terminal", selected: true }).count()) ===
      1 && (await p.getByRole("tab", { name: "Worktrees" }).count()) === 1,
  );
  check(
    "the font size field shows the default",
    (await size.inputValue()) === "13",
  );
  check(
    "the terminal uses the default font size",
    (await termFont(p))?.size === "13px",
    JSON.stringify(await termFont(p)),
  );

  await size.fill("99");
  await size.blur();
  await p.waitForTimeout(300);
  check(
    "an out-of-range size is refused with the reason",
    (await p
      .getByRole("alert")
      .filter({ hasText: "between 8 and 32" })
      .count()) === 1,
  );
  check(
    "a refused value does not touch the terminal",
    (await termFont(p))?.size === "13px",
  );

  await size.fill("abc");
  await size.blur();
  await p.waitForTimeout(200);
  check(
    "text in a number field is caught before saving",
    (await p.getByRole("alert").filter({ hasText: "whole number" }).count()) ===
      1,
  );

  await size.fill("18");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(500);
  check(
    "a valid size clears the error and restyles the open terminal",
    (await p.getByRole("alert").count()) === 0 &&
      (await termFont(p))?.size === "18px",
    JSON.stringify(await termFont(p)),
  );

  const reset = p.getByRole("button", { name: "Reset to default" });
  check("a changed setting offers a reset", (await reset.count()) === 1);
  await reset.click();
  await p.waitForTimeout(500);
  check(
    "reset returns the size to the default",
    (await size.inputValue()) === "13" && (await termFont(p))?.size === "13px",
  );

  const family = p.locator("#terminal-font-family");
  await family.fill("Fira Code, monospace");
  await family.blur();
  await p.waitForTimeout(500);
  const withFamily = await termFont(p);
  check(
    "a configured font goes first, before the bundled ones",
    withFamily !== null &&
      withFamily.family.includes("Fira Code") &&
      withFamily.family.indexOf("Fira Code") <
        withFamily.family.indexOf("JetBrains Mono"),
    JSON.stringify(withFamily),
  );

  await family.fill("Bad; color: red");
  await family.blur();
  await p.waitForTimeout(300);
  check(
    "a font list that tries to inject CSS is refused",
    (await p
      .getByRole("alert")
      .filter({ hasText: "comma-separated" })
      .count()) === 1,
  );

  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 20 }));
  await p.waitForTimeout(500);
  check(
    "an edit made in the file shows up in the field and the terminal",
    (await size.inputValue()) === "20" && (await termFont(p))?.size === "20px",
    `${await size.inputValue()} ${JSON.stringify(await termFont(p))}`,
  );

  await p.getByRole("tab", { name: "Worktrees" }).click();
  const directory = p.locator("#worktrees-directory");
  await directory.fill("relative/path");
  await directory.blur();
  await p.waitForTimeout(300);
  check(
    "a relative worktrees folder is refused",
    (await p
      .getByRole("alert")
      .filter({ hasText: "absolute path" })
      .count()) === 1,
  );
  await directory.fill("~/trees");
  await directory.blur();
  await p.waitForTimeout(300);
  check(
    "a home-relative worktrees folder is accepted",
    (await p.getByRole("alert").count()) === 0,
  );
  await p.getByRole("tab", { name: "Terminal" }).click();
  await p.getByRole("tab", { name: "Worktrees" }).click();
  check(
    "the saved folder is still there after switching sections",
    (await directory.inputValue()) === "~/trees",
  );

  check(
    "the footer shows the config file path",
    (await p.getByText("/home/dev/.lynx/config.json").count()) >= 1,
  );
  await p.getByRole("button", { name: "Open config file" }).click();
  await p.waitForTimeout(200);
  check(
    "Open config file asks the backend to open it",
    await p.evaluate(() => window.__lynxMock.probe.configFileOpened),
  );

  const axeConfig = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations on the settings fields",
    axeConfig.length === 0,
    axeConfig.join(" || "),
  );
});

await session("?mock=broken", 1200, 760, async (p) => {
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(400);
  check(
    "a broken config file is called out",
    (await p
      .getByRole("alert")
      .filter({ hasText: "not valid JSON" })
      .count()) >= 1,
  );
  check(
    "the fields are disabled so nothing overwrites the broken file",
    await p.locator("#terminal-font-size").isDisabled(),
  );
  check(
    "the file path stays visible so it can be fixed",
    (await p.getByText("/home/dev/.lynx/config.json").count()) >= 1,
  );
});

await session(
  "?mock=full",
  1400,
  800,
  async (p) => {
    await p.getByRole("button", { name: /^feat\/auth/ }).click();
    await p.waitForTimeout(500);
    const panel = p.getByRole("complementary", { name: "Side panel" });
    const tree = p.getByRole("tree", {
      name: "Files in the selected worktree",
    });
    const item = (name) => p.getByRole("treeitem", { name });

    check("the side panel starts closed", (await panel.count()) === 0);
    await p.keyboard.press("Control+Shift+E");
    await p.waitForTimeout(500);
    check(
      "the shortcut docks the side panel with the Files tab selected",
      (await panel.count()) === 1 &&
        (await p
          .getByRole("tab", { name: "Files", selected: true })
          .count()) === 1,
    );
    check(
      "the toggle button reports the panel as open",
      (await p
        .getByRole("button", { name: "Toggle side panel" })
        .getAttribute("aria-pressed")) === "true",
    );
    check(
      "the tree lists the worktree's files",
      (await tree.count()) === 1 &&
        (await item(/^src$/).count()) === 1 &&
        (await item(/^README\.md$/).count()) === 1,
    );
    check(
      "git's own folder is not listed",
      (await item(/^\.git$/).count()) === 0,
    );
    check(
      "a folder starts collapsed",
      (await item(/^src$/).getAttribute("aria-expanded")) === "false",
    );

    await item(/^src$/).click();
    await p.waitForTimeout(400);
    check(
      "opening a folder loads and shows its children one level deeper",
      (await item(/^src$/).getAttribute("aria-expanded")) === "true" &&
        (await item(/^App\.tsx$/).count()) === 1 &&
        (await item(/^App\.tsx$/).getAttribute("aria-level")) === "2",
    );
    await item(/^src$/).click();
    await p.waitForTimeout(200);
    check(
      "clicking it again closes it",
      (await item(/^App\.tsx$/).count()) === 0,
    );

    check(
      "ignored files are announced as ignored",
      (await item(/node_modules.*ignored by Git/).count()) === 1,
    );
    await item(/^empty-folder$/).click();
    await p.waitForTimeout(400);
    check(
      "an empty folder says so",
      (await p.getByText("Empty", { exact: true }).count()) === 1,
    );
    await item(/^empty-folder$/).click();

    // Keyboard: the tree behaves like a tree.
    await item(/^docs$/).click();
    await p.waitForTimeout(400);
    await p.keyboard.press("ArrowDown");
    await p.waitForTimeout(120);
    check(
      "ArrowDown moves into the opened folder",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) === "docs/architecture.md",
    );
    await p.keyboard.press("ArrowLeft");
    await p.waitForTimeout(120);
    check(
      "ArrowLeft goes back up to the parent folder",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) === "docs",
    );
    await p.keyboard.press("ArrowLeft");
    await p.waitForTimeout(200);
    check(
      "ArrowLeft on an open folder closes it",
      (await item(/^docs$/).getAttribute("aria-expanded")) === "false",
    );
    await p.keyboard.press("ArrowRight");
    await p.waitForTimeout(300);
    check(
      "ArrowRight opens it again",
      (await item(/^docs$/).getAttribute("aria-expanded")) === "true",
    );
    await p.keyboard.press("End");
    await p.waitForTimeout(120);
    check(
      "End goes to the last row",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) !== "docs",
    );
    await p.keyboard.press("Home");
    await p.waitForTimeout(120);
    check(
      "Home goes to the first row",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) === ".github",
    );
    check(
      "only one row takes the Tab key",
      (await p.locator('[role="treeitem"][tabindex="0"]').count()) === 1,
    );

    // Actions.
    await item(/^README\.md$/).hover();
    await p.getByRole("button", { name: "Actions for README.md" }).click();
    await p.getByRole("menuitem", { name: "Send path to terminal" }).click();
    await p.waitForTimeout(400);
    // Every session stays mounted; the one on screen is the one with a layout box.
    const typed = async () =>
      p.evaluate(
        () =>
          [...document.querySelectorAll(".xterm-rows")].find(
            (el) => el.offsetParent !== null,
          )?.textContent ?? "",
      );
    check(
      "Send path to terminal types the path at the prompt",
      (await typed()).includes("README.md"),
      (await typed()).slice(-120),
    );

    await item(/^a file with spaces\.md$/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Send path to terminal" }).click();
    await p.waitForTimeout(400);
    check(
      "a path with spaces is quoted for the shell",
      (await typed()).includes("'a file with spaces.md'"),
      (await typed()).slice(-160),
    );

    await item(/^docs$/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Copy relative path" }).click();
    await p.waitForTimeout(300);
    check(
      "Copy relative path puts it on the clipboard",
      (await p.evaluate(() => navigator.clipboard.readText())) === "docs",
    );
    await item(/^docs$/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Copy path", exact: true }).click();
    await p.waitForTimeout(300);
    check(
      "Copy path gives the absolute path",
      (await p.evaluate(() => navigator.clipboard.readText())).endsWith(
        "/agent-ade-worktrees/feat-auth/docs",
      ),
      await p.evaluate(() => navigator.clipboard.readText()),
    );
    await item(/^README\.md$/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Open containing folder" }).click();
    await p.waitForTimeout(200);
    check(
      "Open containing folder asks the backend with the worktree and the item",
      (await p.evaluate(() => window.__lynxMock.probe.revealed))?.endsWith(
        "::README.md",
      ) ?? false,
    );

    // The menu is reachable without a mouse.
    await item(/^README\.md$/).focus();
    await p.keyboard.press("Shift+F10");
    await p.waitForTimeout(300);
    check(
      "Shift+F10 opens the actions menu for the focused row",
      (await p
        .getByRole("menuitem", { name: "Copy path", exact: true })
        .count()) === 1,
    );
    await p.keyboard.press("Escape");
    await p.waitForTimeout(400);

    const overflow = await p.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    check(
      "no horizontal overflow with the panel docked",
      overflow <= 0,
      String(overflow),
    );

    const axeFiles = await p.evaluate(async (src) => {
      window.eval(src);
      const r = await window.axe.run(document, {
        runOnly: ["wcag2a", "wcag2aa"],
      });
      return r.violations.map(
        (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
      );
    }, axeSource);
    check(
      "axe: no WCAG A/AA violations in the files panel",
      axeFiles.length === 0,
      axeFiles.join(" || "),
    );

    await p.keyboard.press("Control+Shift+E");
    await p.waitForTimeout(300);
    check("the shortcut closes the panel again", (await panel.count()) === 0);
  },
  { permissions: ["clipboard-read", "clipboard-write"] },
);

await session("?mock=full", 1000, 700, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  check(
    "on a narrower window the panel opens as a drawer over the terminal",
    (await p.getByRole("dialog", { name: "Side panel" }).count()) === 1 &&
      (await p.getByRole("complementary", { name: "Side panel" }).count()) ===
        0,
  );
  check(
    "the drawer shows the same tree",
    (await p.getByRole("treeitem", { name: /^README\.md$/ }).count()) === 1,
  );
  await p.keyboard.press("Escape");
  await p.waitForTimeout(300);
  check(
    "Escape closes the drawer",
    (await p.getByRole("dialog", { name: "Side panel" }).count()) === 0,
  );
});

await session("?mock=empty", 1400, 700, async (p) => {
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(400);
  check(
    "without a worktree the panel says what to do",
    (await p.getByText("Select a worktree to browse its files.").count()) === 1,
  );
});

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(500);
  const terminalTabs = p
    .getByRole("tablist", { name: "Terminals" })
    .getByRole("tab");
  check(
    "the terminal tab list holds only tabs",
    (await p
      .getByRole("tablist", { name: "Terminals" })
      .locator("button:not([role=tab])")
      .count()) === 0,
  );
  check(
    "the worktree starts with two terminal tabs",
    (await terminalTabs.count()) === 2,
  );

  await terminalTabs.first().focus();
  await p.keyboard.press("Delete");
  await p.waitForTimeout(300);
  check(
    "Delete closes the focused terminal tab",
    (await terminalTabs.count()) === 1,
  );

  await terminalTabs.first().hover();
  await terminalTabs.first().locator("[data-close]").click();
  await p.waitForTimeout(300);
  check(
    "clicking the x closes a tab and leaves the worktree with the launcher",
    (await terminalTabs.count()) === 0 &&
      (await p.getByText("Nothing is running in feat/auth").count()) === 1,
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
      (await p.getByRole("heading", { name: "Projects" }).count()) === 0,
  );
  check("narrow window has no horizontal overflow", (await overflow(p)) <= 0);
  await p.keyboard.press("Control+Shift+B");
  await p.waitForTimeout(400);
  check(
    "Ctrl+Shift+B opens the drawer",
    (await p.getByRole("heading", { name: "Projects" }).count()) >= 1,
  );
  check(
    "the drawer carries the primary bar with settings",
    (await p
      .getByRole("navigation", { name: "Primary navigation" })
      .count()) === 1,
  );
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  check(
    "choosing a worktree closes the drawer",
    (await p.getByRole("heading", { name: "Projects" }).count()) === 0,
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
