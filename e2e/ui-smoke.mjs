/**
 * Drives the interface in headless Chrome against the in-memory backend
 * (`?mock=`), so no desktop window is involved. Run with `bun run e2e`.
 * Set CHROME_PATH when Chrome is not at a standard location.
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
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

// Run vite with node itself so killing the child also stops the server.
const viteBin = join(
  dirname(require.resolve("vite/package.json")),
  "bin",
  "vite.js",
);
const vite = spawn(
  process.execPath,
  [viteBin, "--port", String(PORT), "--strictPort"],
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

// The DOM renderer keeps terminal text in the page, which many checks read. So sessions
// run without WebGL unless they ask for it; the WebGL session below covers that path.
async function session(query, width, height, run, contextOptions = {}) {
  const { webgl = false, ...options } = contextOptions;
  const context = await browser.newContext({
    viewport: { width, height },
    ...options,
  });
  const page = await context.newPage();
  if (!webgl) {
    await page.addInitScript(() => {
      const getContext = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (type, ...rest) {
        return /^webgl/.test(type)
          ? null
          : getContext.call(this, type, ...rest);
      };
    });
  }
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

// Under WebGL the terminal's text is drawn on a canvas, so the page cannot read it.
// With copy-on-select on, a drag over the screen copies the buffer text instead.
const setTerminalSwitch = async (p, id, on) => {
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Behavior", exact: true }).click();
  const toggle = p.locator(`#${id}`);
  if ((await toggle.getAttribute("aria-checked")) !== String(on)) {
    await toggle.click();
  }
  await p.waitForTimeout(300);
  await p.getByRole("button", { name: "Close settings" }).click();
  await p.waitForTimeout(300);
};
const selectTerminalScreen = async (p) => {
  const box = await p.evaluate(() => {
    const screen = [...document.querySelectorAll(".xterm-screen")].find(
      (el) => el.offsetParent !== null,
    );
    const r = screen.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  });
  await p.mouse.move(box.x + 2, box.y + 2);
  await p.mouse.down();
  await p.mouse.move(box.x + box.width - 2, box.y + box.height - 2, {
    steps: 8,
  });
  await p.mouse.up();
  await p.waitForTimeout(200);
};
const readTerminal = async (p) => {
  await p.evaluate(() => {
    window.__copied = [];
    navigator.clipboard.writeText = async (text) => {
      window.__copied.push(text);
    };
  });
  await setTerminalSwitch(p, "terminal-copy-on-select", true);
  await selectTerminalScreen(p);
  const text = await p.evaluate(() => window.__copied.at(-1) ?? "");
  await setTerminalSwitch(p, "terminal-copy-on-select", false);
  // Later checks read the real clipboard, so the stub must not outlive this read.
  await p.evaluate(() => delete navigator.clipboard.writeText);
  return text;
};

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

  // The tree reads top to bottom: Work > Client X > api-server. Arrow keys walk it.
  await p.getByRole("button", { name: "Collapse Work" }).focus();
  await p.keyboard.press("ArrowDown");
  await p.waitForTimeout(120);
  const onClientX = await p.evaluate(
    () => document.activeElement?.textContent ?? "",
  );
  check(
    "ArrowDown moves from a folder to the folder inside it",
    onClientX.includes("Client X"),
    onClientX,
  );
  await p.keyboard.press("ArrowDown");
  await p.waitForTimeout(120);
  const onRepo = await p.evaluate(() =>
    document.activeElement?.getAttribute("data-project-id"),
  );
  check(
    "and on to the repo inside that folder",
    onRepo === "r2",
    String(onRepo),
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

  // There are no DOM rows under WebGL; the screen box holds every row of output.
  const lastRowVisible = await p.evaluate(() => {
    const screen = [...document.querySelectorAll(".xterm-screen")].find(
      (el) => el.offsetParent !== null,
    );
    const holder = screen?.closest(".xterm")?.parentElement;
    if (!screen || !holder) return false;
    return (
      screen.getBoundingClientRect().bottom <=
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

// The font the visible terminal was given. WebGL draws the text on a canvas, so
// TerminalView mirrors the font on its host element where the page can read it.
const termFont = (p) =>
  p.evaluate(() => {
    const host = [
      ...document.querySelectorAll("[data-terminal-font-size]"),
    ].find((el) => el.offsetParent !== null);
    return host
      ? {
          size: `${host.dataset.terminalFontSize}px`,
          family: host.dataset.terminalFontFamily,
        }
      : null;
  });

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(600);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(400);

  check(
    "the terminal settings are split into Text, Appearance and Behavior tabs",
    (await p.getByRole("tab", { name: "Text", exact: true }).count()) === 1 &&
      (await p
        .getByRole("tab", { name: "Appearance", exact: true })
        .count()) === 1 &&
      (await p.getByRole("tab", { name: "Behavior", exact: true }).count()) ===
        1,
  );

  const size = p.locator("#terminal-font-size");
  const sizeText = async () => ((await size.textContent()) ?? "").trim();
  const increase = p.getByRole("button", { name: "Increase" });
  const decrease = p.getByRole("button", { name: "Decrease" });
  check(
    "settings lists the sections and starts on Terminal",
    (await p.getByRole("tab", { name: "Terminal", selected: true }).count()) ===
      1 && (await p.getByRole("tab", { name: "Worktrees" }).count()) === 1,
  );
  check("the font size shows the default", (await sizeText()) === "13");
  check(
    "the terminal uses the default font size",
    (await termFont(p))?.size === "13px",
    JSON.stringify(await termFont(p)),
  );

  await increase.click();
  await p.waitForTimeout(500);
  check(
    "a step up saves the size and restyles the open terminal",
    (await sizeText()) === "14" && (await termFont(p))?.size === "14px",
    `${await sizeText()} ${JSON.stringify(await termFont(p))}`,
  );
  const reset = p.getByRole("button", { name: "Reset to default" });
  check("a changed setting offers a reset", (await reset.count()) === 1);
  await reset.click();
  await p.waitForTimeout(500);
  check(
    "reset returns the size to the default",
    (await sizeText()) === "13" && (await termFont(p))?.size === "13px",
  );

  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 32 }));
  await p.waitForTimeout(400);
  check("the larger button stops at the maximum", await increase.isDisabled());
  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 8 }));
  await p.waitForTimeout(400);
  check("the smaller button stops at the minimum", await decrease.isDisabled());
  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 20 }));
  await p.waitForTimeout(400);
  check(
    "an edit made in the file shows up in the stepper and the terminal",
    (await sizeText()) === "20" && (await termFont(p))?.size === "20px",
    `${await sizeText()} ${JSON.stringify(await termFont(p))}`,
  );

  const family = p.locator("#terminal-font-family");
  await family.click();
  await p.getByRole("option", { name: "Fira Code" }).click();
  await p.waitForTimeout(500);
  const withFamily = await termFont(p);
  check(
    "a preset font goes first, before the bundled ones",
    withFamily !== null &&
      withFamily.family.includes("Fira Code") &&
      withFamily.family.indexOf("Fira Code") <
        withFamily.family.indexOf("JetBrains Mono"),
    JSON.stringify(withFamily),
  );

  await family.click();
  await p.getByRole("option", { name: "Custom…" }).click();
  const typed = p.getByRole("textbox", { name: "Font", exact: true });
  check("Custom opens a box for a font list", (await typed.count()) === 1);
  await typed.fill("Bad; color: red");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(300);
  check(
    "a font list that tries to inject CSS is refused",
    (await p
      .getByRole("alert")
      .filter({ hasText: "comma-separated" })
      .count()) === 1,
  );
  await typed.fill("Hack, monospace");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(500);
  check(
    "a typed font list is saved and goes first",
    (await termFont(p))?.family.includes("Hack") === true &&
      (await p.getByRole("alert").count()) === 0,
  );
  await family.click();
  await p.getByRole("option", { name: /JetBrains Mono/ }).click();
  await p.waitForTimeout(400);
  check(
    "choosing the bundled font puts the default back",
    !((await termFont(p))?.family ?? "").includes("Hack"),
  );

  await p.getByRole("tab", { name: "Appearance", exact: true }).click();
  const terminalFrame = () =>
    p.evaluate(() => {
      const visible = [...document.querySelectorAll(".xterm")].find(
        (x) => x.offsetParent !== null,
      );
      const frame = visible?.parentElement?.parentElement;
      return frame ? getComputedStyle(frame).backgroundColor : null;
    });
  const pick = async (id, option) => {
    await p.locator(`#${id}`).click();
    await p.getByRole("option", { name: option, exact: true }).click();
    await p.waitForTimeout(400);
  };
  const textChoice = async () =>
    (await p.locator("#terminal-text-colours").textContent()) ?? "";

  await pick("terminal-color-scheme", "Light");
  check(
    "a theme repaints the terminal background",
    (await terminalFrame()) === "rgb(250, 250, 250)",
    String(await terminalFrame()),
  );
  check(
    "the text colours follow the theme until another theme is picked",
    (await textChoice()).includes("Same as the theme"),
  );

  await pick("terminal-text-colours", "Nord");
  check(
    "the text colours can come from another theme",
    (await textChoice()).includes("Nord"),
  );
  check(
    "the background stays the chosen theme's when the text borrows another",
    (await terminalFrame()) === "rgb(250, 250, 250)",
    String(await terminalFrame()),
  );

  await pick("terminal-color-scheme", "Dracula");
  check(
    "changing the theme changes the background and keeps the borrowed text",
    (await terminalFrame()) === "rgb(40, 42, 54)" &&
      (await textChoice()).includes("Nord"),
    String(await terminalFrame()),
  );

  await pick("terminal-text-colours", "Same as the theme");
  check(
    "same as the theme brings the text colours back to the theme's",
    (await textChoice()).includes("Same as the theme") &&
      (await p.getByRole("alert").count()) === 0,
  );

  // Terminal text is drawn in the colours the user picked, so it is left out of the check.
  await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(
      { exclude: [".xterm"] },
      { runOnly: ["wcag2a", "wcag2aa"] },
    );
    window.__appearanceAxe = r.violations.map(
      (v) => `${v.id} ${v.nodes.length}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations on the appearance tab",
    (await p.evaluate(() => window.__appearanceAxe.length)) === 0,
    await p.evaluate(() => window.__appearanceAxe.join(", ")),
  );

  await p.getByRole("tab", { name: "Behavior", exact: true }).click();
  const shell = p.locator("#terminal-shell");
  await shell.click();
  check(
    "the shell list offers the shells found on the machine",
    (await p.getByRole("option", { name: "/bin/zsh" }).count()) === 1,
  );
  await p.getByRole("option", { name: "/bin/zsh" }).click();
  await p.waitForTimeout(400);
  check(
    "the chosen shell is shown and saved without an error",
    ((await shell.textContent()) ?? "").includes("/bin/zsh") &&
      (await p.getByRole("alert").count()) === 0,
  );

  await p.getByRole("tab", { name: "Text", exact: true }).click();
  const scrollback = p.locator("#terminal-scrollback");
  await scrollback.click();
  await p.getByRole("option", { name: /10[,.]000/ }).click();
  await p.waitForTimeout(400);
  check(
    "scrollback is picked from a list of sizes",
    /10[,.]000/.test((await scrollback.textContent()) ?? ""),
  );

  await p.getByRole("tab", { name: "Behavior", exact: true }).click();
  const copyOnSelect = p.locator("#terminal-copy-on-select");
  check(
    "switches start off",
    (await copyOnSelect.getAttribute("aria-checked")) === "false",
  );
  await copyOnSelect.click();
  await p.waitForTimeout(300);
  check(
    "a switch turns on and saves",
    (await copyOnSelect.getAttribute("aria-checked")) === "true",
  );
  check(
    "Option as Meta is only offered on macOS",
    (await p.locator("#terminal-option-as-meta").count()) === 0,
  );

  const bell = p.locator("#terminal-bell");
  check(
    "the bell flashes the terminal by default",
    /Flash/.test((await bell.textContent()) ?? ""),
  );
  await bell.click();
  await p.getByRole("option", { name: "Nothing" }).click();
  await p.waitForTimeout(300);
  check(
    "the bell can be set to nothing",
    /Nothing/.test((await bell.textContent()) ?? ""),
  );

  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 13 }));
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
  await p.getByRole("button", { name: "Choose folder…" }).click();
  await p.waitForTimeout(400);
  check(
    "Choose folder saves the folder that was picked",
    (await directory.inputValue()) === "/home/dev/web-app",
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
    const r = await window.axe.run(
      { exclude: [".xterm"] },
      {
        runOnly: ["wcag2a", "wcag2aa"],
      },
    );
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

await session("?mock=full", 1200, 760, async (p) => {
  await p.evaluate(() => {
    window.__copied = [];
    navigator.clipboard.writeText = async (text) => {
      window.__copied.push(text);
    };
  });
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.getByRole("tab", { name: /claude/ }).click();
  await p.waitForTimeout(500);

  const dragAcrossFiller = () => selectTerminalScreen(p);

  await setTerminalSwitch(p, "terminal-copy-on-select", true);
  await dragAcrossFiller();
  const copied = await p.evaluate(() => window.__copied.at(-1) ?? "");
  check(
    "with copy on release, a selection reaches the clipboard",
    copied.includes("filler"),
    copied,
  );
  await setTerminalSwitch(p, "terminal-copy-on-select", false);
  const copiesBefore = await p.evaluate(() => window.__copied.length);
  await dragAcrossFiller();
  check(
    "with copy on release off, a selection is not copied",
    (await p.evaluate(() => window.__copied.length)) === copiesBefore,
  );

  await p.locator(".xterm-helper-textarea:visible").focus();
  await p.keyboard.press("Control+g");
  await p.waitForTimeout(100);
  check(
    "a bell flashes the terminal by default",
    (await p.locator(".terminal-bell").count()) === 1,
  );
  await p.waitForTimeout(600);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Behavior", exact: true }).click();
  await p.locator("#terminal-bell").click();
  await p.getByRole("option", { name: "Nothing" }).click();
  await p.getByRole("button", { name: "Close settings" }).click();
  await p.waitForTimeout(300);
  await p.locator(".xterm-helper-textarea:visible").focus();
  await p.keyboard.press("Control+g");
  await p.waitForTimeout(100);
  check(
    "with the bell set to nothing, nothing flashes",
    (await p.locator(".terminal-bell").count()) === 0,
  );

  await setTerminalSwitch(p, "terminal-confirm-close", true);
  await p
    .getByRole("tab", { name: /claude/ })
    .locator("[data-close]")
    .click();
  await p.waitForTimeout(300);
  check(
    "closing a tab asks first when confirmation is on",
    (await p.getByRole("alertdialog").count()) === 1,
  );
  await p.getByRole("button", { name: "Keep it open" }).click();
  await p.waitForTimeout(300);
  check(
    "keeping it open leaves the tab where it was",
    (await p.getByRole("tab", { name: /claude/ }).count()) === 1 &&
      (await p.getByRole("alertdialog").count()) === 0,
  );
  await p
    .getByRole("tab", { name: /claude/ })
    .locator("[data-close]")
    .click();
  await p.waitForTimeout(300);
  await p.getByRole("button", { name: "Close terminal" }).click();
  await p.waitForTimeout(400);
  check(
    "confirming closes the tab",
    (await p.getByRole("tab", { name: /claude/ }).count()) === 0,
  );

  await setTerminalSwitch(p, "terminal-close-on-exit", true);
  await p.evaluate(() => window.__lynxMock.endAllTerminals());
  await p.waitForTimeout(500);
  check(
    "with close on exit, the tabs of programs that ended are closed",
    (await p
      .getByRole("tablist", { name: "Terminals" })
      .getByRole("tab")
      .count()) === 0,
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
    await p.getByRole("button", { name: "Increase" }).isDisabled(),
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
    // Every session stays mounted; readTerminal reads the one on screen.
    const typed = await readTerminal(p);
    check(
      "Send path to terminal types the path at the prompt",
      typed.includes("README.md"),
      typed.slice(-120),
    );

    await item(/^a file with spaces\.md$/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Send path to terminal" }).click();
    await p.waitForTimeout(400);
    const typedWithSpaces = await readTerminal(p);
    check(
      "a path with spaces is quoted for the shell",
      typedWithSpaces.includes("'a file with spaces.md'"),
      typedWithSpaces.slice(-160),
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

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.getByRole("tab", { name: /claude/ }).click();
  await p.waitForTimeout(500);
  // Other sessions stay mounted under the visible one: a click must still reach the visible terminal.
  await p.evaluate(() => document.activeElement?.blur());
  const box = await p.locator(".xterm-screen:visible").first().boundingBox();
  await p.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await p.waitForTimeout(200);
  check(
    "clicking the terminal focuses it so the keyboard can type",
    await p.evaluate(
      () =>
        document.activeElement?.classList.contains("xterm-helper-textarea") ??
        false,
    ),
  );
});

await session(
  "?mock=full",
  1200,
  760,
  async (p) => {
    await p.getByRole("button", { name: /^feat\/auth/ }).click();
    await p.getByRole("tab", { name: /claude/ }).click();
    await p.waitForTimeout(600);
    const renderer = () =>
      p.evaluate(() => {
        const visible = [...document.querySelectorAll(".xterm")].find(
          (x) => x.offsetParent !== null,
        );
        return {
          canvas: visible
            ? [...visible.querySelectorAll("canvas")].some(
                (c) => c.className === "",
              )
            : false,
          domRows: visible ? visible.querySelectorAll(".xterm-rows").length : 0,
        };
      });
    const drawn = await renderer();
    check(
      "with WebGL the terminal draws on a canvas, not DOM rows",
      drawn.canvas && drawn.domRows === 0,
      JSON.stringify(drawn),
    );

    // Losing the GPU context must hand the terminal back to the DOM renderer.
    await p.evaluate(() => {
      const visible = [...document.querySelectorAll(".xterm")].find(
        (x) => x.offsetParent !== null,
      );
      const canvas = [...visible.querySelectorAll("canvas")].find(
        (c) => c.className === "",
      );
      canvas
        .getContext("webgl2")
        .getExtension("WEBGL_lose_context")
        .loseContext();
    });
    // The addon waits 3 s for the context to come back before it hands over to the DOM renderer.
    await p
      .waitForFunction(
        () => {
          const visible = [...document.querySelectorAll(".xterm")].find(
            (x) => x.offsetParent !== null,
          );
          return visible?.querySelectorAll(".xterm-rows").length > 0;
        },
        null,
        { timeout: 6000 },
      )
      .catch(() => undefined);
    const fallback = await renderer();
    check(
      "losing the WebGL context falls back to the DOM renderer",
      fallback.domRows > 0,
      JSON.stringify(fallback),
    );
  },
  { webgl: true },
);

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

await session("?mock=full", 1300, 760, async (p) => {
  const agent = (name, options = {}) =>
    p.getByRole("button", { name, ...options });
  const agentRows = p.locator("[data-agent-id]");
  const groupOf = (path) => p.locator(`[data-agent-group="${path}"]`);
  const toggleGroup = (path) => groupOf(path).locator(":scope > button");
  const featPath = "/home/dev/agent-ade-worktrees/feat-auth";
  const fixPath = "/home/dev/agent-ade-worktrees/fix-pty";
  const refactorPath = "/home/dev/api-server-worktrees/refactor-db";

  check(
    "every worktree with agents keeps them listed, collapsed to one line",
    (await p.locator("[data-agent-group]").count()) === 3 &&
      (await p
        .locator('[data-agent-group] > button[aria-expanded="false"]')
        .count()) === 3 &&
      (await agentRows.count()) === 0,
  );
  check(
    "a collapsed group says how many agents it has and how many are running",
    (await agent("1 agent, 1 running").count()) === 2 &&
      (await agent("1 agent", { exact: true }).count()) === 1,
  );

  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  check(
    "selecting a worktree does not hide the other worktrees' agents",
    (await p.locator("[data-agent-group]").count()) === 3,
  );

  await toggleGroup(featPath).click();
  await p.waitForTimeout(300);
  check(
    "opening a group lists its agents",
    (await agent(/^claude, running$/).count()) === 1,
  );
  check(
    "plain shells are not listed as agents",
    (await agentRows.count()) === 1,
  );
  check(
    "an agent row says how long ago it started",
    ((await agent(/^claude, running$/).textContent()) ?? "").includes("5m"),
  );
  check(
    "another worktree's agents stay collapsed until opened",
    (await agent(/^codex/).count()) === 0,
  );

  await p.getByRole("tab", { name: /zsh/ }).click();
  await agent(/^claude, running$/).click();
  await p.waitForTimeout(400);
  check(
    "clicking an agent brings its tab forward",
    (await p.getByRole("tab", { name: /claude/, selected: true }).count()) ===
      1,
  );
  check(
    "and focuses its terminal",
    await p.evaluate(() => document.activeElement?.closest(".xterm") !== null),
  );
  check(
    "the agent row of the open tab is marked current",
    (await agent(/^claude, running$/).getAttribute("aria-current")) === "true",
  );

  await p.getByRole("button", { name: /^feat\/auth/ }).focus();
  await p.keyboard.press("ArrowDown");
  await p.waitForTimeout(120);
  check(
    "ArrowDown walks from the worktree into its agents group",
    (await p.evaluate(() =>
      document.activeElement
        ?.closest("[data-agent-group]")
        ?.getAttribute("data-agent-group"),
    )) === featPath,
  );

  const axeAgents = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations with agent rows",
    axeAgents.length === 0,
    axeAgents.join(" || "),
  );

  await p.getByRole("button", { name: /^fix\/pty/ }).click();
  await p.waitForTimeout(400);
  check(
    "selecting another worktree keeps the first one's agents in view",
    (await agent(/^claude, running$/).count()) === 1,
  );
  check(
    "the newly selected worktree's agents stay collapsed until opened",
    (await agent(/^codex, running$/).count()) === 0,
  );
  await toggleGroup(fixPath).click();
  await p.waitForTimeout(300);
  check(
    "each row shows its own age",
    ((await agent(/^codex, running$/).textContent()) ?? "").includes("3h"),
  );

  await toggleGroup(refactorPath).click();
  await p.waitForTimeout(300);
  check(
    "a finished agent is shown as exited, with its age",
    (await agent(/^claude, exited with code 0$/).count()) === 1 &&
      ((await agent(/^claude, exited/).textContent()) ?? "").includes("2d"),
  );

  await agent(/^claude, running$/).hover();
  await groupOf(featPath).getByRole("button", { name: "Close claude" }).click();
  await p.waitForTimeout(400);
  check(
    "closing from the row removes the agent and its tab",
    (await agent(/^claude, running$/).count()) === 0 &&
      (await p.getByRole("tab", { name: /claude/ }).count()) === 0,
  );
  check(
    "a group with no agents left disappears",
    (await groupOf(featPath).count()) === 0,
  );
});

await session("?mock=full", 1300, 760, async (p) => {
  const panel = p.getByRole("complementary", { name: "Projects" });
  const longRow = p.getByRole("button", { name: /^feature\/very-long-branch/ });
  const fits = async (row) =>
    p.evaluate(
      ([rowEl, panelEl]) =>
        rowEl.getBoundingClientRect().right <=
        panelEl.getBoundingClientRect().right + 0.5,
      [await row.elementHandle(), await panel.elementHandle()],
    );
  check(
    "a long branch name stays inside the sidebar (it truncates)",
    await fits(longRow),
  );

  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(300);
  await p
    .locator(
      '[data-agent-group="/home/dev/agent-ade-worktrees/feat-auth"] > button',
    )
    .click();
  await p.waitForTimeout(300);
  const agentRow = p.getByRole("button", { name: /^claude, running$/ });
  check("agent rows stay inside the sidebar too", await fits(agentRow));

  const surfaces = await p.evaluate(() => {
    const bg = (el) => getComputedStyle(el).backgroundColor;
    const open = document.querySelector('[data-agent-id][aria-current="true"]');
    const idle = [...document.querySelectorAll("[data-agent-id]")].find(
      (el) => el.getAttribute("aria-current") !== "true",
    );
    const card = document
      .querySelector(
        '[aria-current="true"][data-nav-item]:not([data-agent-id])',
      )
      ?.closest("li");
    return {
      open: open ? bg(open) : null,
      idle: idle ? bg(idle) : null,
      card: card ? bg(card) : null,
      ring: card ? getComputedStyle(card).boxShadow : null,
    };
  });
  const indent = await p.evaluate(() => {
    const header = document.querySelector(
      '[aria-current="true"][data-nav-item]:not([data-agent-id])',
    );
    const dot = header?.querySelector(".rounded-full");
    const pill = document.querySelector("[data-agent-id]");
    if (!dot || !pill) return null;
    return {
      dotLeft: dot.getBoundingClientRect().left,
      pillLeft: pill.getBoundingClientRect().left,
    };
  });
  check(
    "the agent pill starts at the worktree's dot, indented from the card's edge",
    indent !== null && Math.abs(indent.pillLeft - indent.dotLeft) <= 1,
    JSON.stringify(indent),
  );
  const hierarchy = await p.evaluate(() => {
    const worktreeLabel = document.querySelector(
      '[aria-current="true"][data-nav-item]:not([data-agent-id]) span[title]',
    );
    const agentTitle = document.querySelector(
      "[data-agent-id] > span.truncate",
    );
    const agentIcons = [
      ...document.querySelectorAll("[data-agent-id] > svg"),
    ].map((svg) => svg.getBoundingClientRect().width);
    return {
      worktreeFont: worktreeLabel
        ? parseFloat(getComputedStyle(worktreeLabel).fontSize)
        : null,
      agentFont: agentTitle
        ? parseFloat(getComputedStyle(agentTitle).fontSize)
        : null,
      agentIcons,
    };
  });
  check(
    "agent titles are a step smaller than the worktree name",
    hierarchy.agentFont !== null &&
      hierarchy.worktreeFont !== null &&
      hierarchy.agentFont < hierarchy.worktreeFont,
    JSON.stringify(hierarchy),
  );
  check(
    "agent icons are smaller than the worktree name's text",
    hierarchy.agentIcons.length > 0 &&
      hierarchy.agentIcons.every((w) => w < hierarchy.worktreeFont),
    JSON.stringify(hierarchy),
  );
  check(
    "the card has no border or ring",
    surfaces.ring === "none" || surfaces.ring === "",
    String(surfaces.ring),
  );
  check(
    "an agent that is not open has no surface of its own",
    surfaces.idle === null || surfaces.idle === "rgba(0, 0, 0, 0)",
    String(surfaces.idle),
  );
});

await session("?mock=full", 1300, 760, async (p) => {
  const tree = p.getByRole("complementary", { name: "Projects" });
  const folderItems = () => p.locator("[data-folder-id]");
  const inside = (folderName, selector) =>
    folderItems().filter({ hasText: folderName }).first().locator(selector);
  const names = () =>
    folderItems().evaluateAll((items) =>
      items.map((el) => el.querySelector("button")?.textContent?.trim()),
    );
  // A row is reached through its header (named "Collapse X" or "Expand X"), which is what hovering reveals the menu from.
  const headerOf = (name) =>
    tree
      .getByRole("button", { name: new RegExp(`^(Collapse|Expand) ${name}$`) })
      .first();
  const openMenu = async (name, action, isFolder = false) => {
    await headerOf(name).hover();
    await tree
      .getByRole("button", {
        name: isFolder ? `Actions for folder ${name}` : `Actions for ${name}`,
      })
      .first()
      .click();
    await p.getByRole("menuitem", { name: action }).click();
  };

  check(
    "the seeded tree nests Client X inside Work",
    (await inside("Work", '[data-folder-id="f-client"]').count()) === 1,
  );
  check(
    "api-server sits inside Client X",
    (await p
      .locator('[data-folder-id="f-client"] [data-project-id="r2"]')
      .count()) === 1,
  );
  check(
    "folders come before repos at the same level",
    await p.evaluate(() => {
      const first = document.querySelector("[data-folder-id]");
      const agent = document.querySelector('[data-project-id="r1"]');
      return (
        !!first &&
        !!agent &&
        (first.compareDocumentPosition(agent) &
          Node.DOCUMENT_POSITION_FOLLOWING) !==
          0
      );
    }),
  );

  // A top-level folder is a group label: no icon. A folder inside another keeps its icon.
  const headerIcons = await p.evaluate(() => ({
    topLevel: document.querySelectorAll(
      '[data-folder-id="f-work"] > div > button svg',
    ).length,
    nested: document.querySelectorAll(
      '[data-folder-id="f-client"] > div > button svg',
    ).length,
  }));
  check(
    "a top-level folder reads as a group label, with no icon, and a nested one keeps its icon",
    headerIcons.topLevel === 0 && headerIcons.nested === 1,
    JSON.stringify(headerIcons),
  );

  // The disclosure arrow moved from the left of the row into its hover actions.
  const chevrons = await p.evaluate(() => ({
    inActions: document.querySelectorAll(
      '[data-folder-id] > div > div svg[data-icon="chevron-right"]',
    ).length,
    inHeaders: document.querySelectorAll(
      '[data-folder-id] > div > button svg[data-icon="chevron-right"], button[data-project-id] svg[data-icon="chevron-right"]',
    ).length,
  }));
  check(
    "the disclosure arrow sits in the row's actions, not on the left",
    chevrons.inActions > 0 && chevrons.inHeaders === 0,
    JSON.stringify(chevrons),
  );

  await p.mouse.move(0, 0);
  await p.waitForTimeout(300);
  const actionOpacity = () =>
    p.evaluate(
      () =>
        getComputedStyle(
          document.querySelector('[data-folder-id="f-work"] > div > div'),
        ).opacity,
    );
  const hiddenBefore = await actionOpacity();
  await p.locator('[data-folder-id="f-work"] > div > button').first().hover();
  await p.waitForTimeout(300);
  const shownAfter = await actionOpacity();
  check(
    "the arrow appears only when the row is hovered",
    hiddenBefore === "0" && shownAfter === "1",
    `${hiddenBefore} -> ${shownAfter}`,
  );

  const clickPassesThrough = await p.evaluate(() => {
    const arrow = document.querySelector(
      '[data-folder-id="f-work"] > div > div svg[data-icon="chevron-right"]',
    );
    const box = arrow.getBoundingClientRect();
    const under = document.elementFromPoint(
      box.left + box.width / 2,
      box.top + box.height / 2,
    );
    return !!under?.closest('[data-folder-id="f-work"] > div > button');
  });
  check(
    "clicking the arrow's area still toggles the row underneath",
    clickPassesThrough,
  );

  await p.getByRole("button", { name: "Collapse Work" }).click();
  await p.waitForTimeout(200);
  check(
    "collapsing a folder hides everything inside it",
    (await p.locator('[data-project-id="r2"]').count()) === 0,
  );
  await p.getByRole("button", { name: "Expand Work" }).click();
  await p.waitForTimeout(200);
  check(
    "expanding it brings the contents back",
    (await p.locator('[data-project-id="r2"]').count()) === 1,
  );

  // New folder at the top level: the header button, not the one on a folder row.
  await tree
    .getByRole("button", { name: "New folder", exact: true })
    .first()
    .click();
  await p.locator("#folder-name").fill("Personal");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(300);
  check(
    "a new top-level folder sorts before Work",
    (await names())[0] === "Personal",
    JSON.stringify(await names()),
  );

  // New folder inside Work: created from Work's own row.
  await folderItems()
    .filter({ hasText: "Work" })
    .first()
    .getByRole("button", { name: "New folder", exact: true })
    .first()
    .click();
  await p.locator("#folder-name").fill("Deep");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(300);
  check(
    "a folder made inside Work lands inside Work",
    (await inside("Work", '[data-folder-id]:has-text("Deep")').count()) >= 1,
  );

  // A folder cannot be moved into itself or into its own folders.
  await tree
    .getByRole("button", { name: "Actions for Work" })
    .first()
    .click({ force: true })
    .catch(() => {});
  await openMenu("Work", "Move to folder…", true);
  await p.waitForTimeout(200);
  const blocked = async (id) => p.locator(`[data-option="${id}"]`).isDisabled();
  check("Work cannot be moved inside itself", await blocked("f-work"));
  check(
    "Work cannot be moved inside Client X either",
    await blocked("f-client"),
  );
  check("the top level is always a valid place", !(await blocked("top")));
  await p.keyboard.press("Escape");
  await p.waitForTimeout(200);

  // Deleting a folder moves what was inside it up one level; nothing is lost.
  await openMenu("Client X", "Delete folder…", true);
  await p.waitForTimeout(200);
  check(
    "the delete dialog says contents move up",
    (await p
      .getByText("Its folders and projects move up one level", { exact: false })
      .count()) === 1,
  );
  await p.getByRole("button", { name: "Delete folder", exact: true }).click();
  await p.waitForTimeout(300);
  check(
    "api-server is still listed after its folder is gone",
    (await p.locator('[data-project-id="r2"]').count()) === 1,
  );
  check(
    "and it moved up into Work",
    (await p
      .locator('[data-folder-id="f-work"] [data-project-id="r2"]')
      .count()) === 1,
  );

  // Moving a repo back to the top level.
  await openMenu("api-server", "Move to folder…");
  await p.waitForTimeout(200);
  await p.locator('[data-option="top"]').click();
  await p.getByRole("button", { name: "Move here" }).click();
  await p.waitForTimeout(300);
  check(
    "moving a repo to the top level takes it out of its folder",
    (await p.locator('[data-folder-id] [data-project-id="r2"]').count()) ===
      0 && (await p.locator('[data-project-id="r2"]').count()) === 1,
  );

  // Renaming keeps the place.
  await openMenu("Work", "Rename…", true);
  await p.locator("#folder-name").fill("Trabalho");
  await p.keyboard.press("Enter");
  await p.waitForTimeout(300);
  check(
    "renaming a folder keeps its contents where they were",
    (await p.locator('[data-folder-id]:has-text("Trabalho")').count()) >= 1,
  );

  const axeTree = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations in the folder tree",
    axeTree.length === 0,
    axeTree.join(" || "),
  );
});

await session("?mock=full", 1300, 760, async (p) => {
  const repoHeader = p.locator('[data-project-id="r1"]');
  const spokenOf = async (header) => {
    const id = await header.getAttribute("aria-describedby");
    return id ? p.locator(`#${id}`).textContent() : null;
  };

  check(
    "an expanded repo has nothing to say about what it hides",
    (await spokenOf(repoHeader)) === null,
  );

  await p.getByRole("button", { name: "Collapse agent-ade" }).click();
  await p.waitForTimeout(200);
  const spoken = await spokenOf(repoHeader);
  check(
    "a collapsed repo says what it hides, including the agents running in it",
    spoken === "3 worktrees, 2 agents running",
    spoken ?? "null",
  );
  const shownInRow =
    (await repoHeader.locator('span[aria-hidden="true"]').textContent()) ?? "";
  check(
    "the row shows the worktree count and the running agents at its right edge",
    shownInRow.includes("3 worktrees") && shownInRow.endsWith("2"),
    shownInRow,
  );
  await p.getByRole("button", { name: "Expand agent-ade" }).click();
  await p.waitForTimeout(200);
  check(
    "expanding the repo removes the summary again",
    (await spokenOf(repoHeader)) === null,
  );

  await p.getByRole("button", { name: "Collapse Work" }).click();
  await p.waitForTimeout(200);
  const folderSpoken = await spokenOf(
    p.locator('[data-folder-id="f-work"] > div > button'),
  );
  check(
    "a collapsed folder counts the projects inside it, at any depth",
    folderSpoken === "1 project",
    folderSpoken ?? "null",
  );
  await p.getByRole("button", { name: "Expand Work" }).click();
  await p.waitForTimeout(200);
  check(
    "the folder's name is still what the header is called",
    (await p.getByRole("button", { name: "Collapse Work" }).count()) === 1,
  );
});

await session("?mock=crowded", 1300, 760, async (p) => {
  const fixGroup = p.locator(
    '[data-agent-group="/home/dev/agent-ade-worktrees/fix-pty"]',
  );
  const header = fixGroup.locator(":scope > button");
  const badges = header.locator('svg[data-icon="sparkles"]');

  check(
    "a collapsed group shows up to three agent badges side by side",
    (await badges.count()) === 3,
    String(await badges.count()),
  );
  check(
    "and a +X badge for the agents that do not fit",
    (await header.getByText("+1", { exact: true }).count()) === 1,
  );
  check(
    "the group names all four agents and that they are running",
    (await header.getAttribute("aria-label")) === "4 agents, 4 running",
    String(await header.getAttribute("aria-label")),
  );

  await header.click();
  await p.waitForTimeout(300);
  check(
    "opened, the header is only the word Agents, with no badges",
    (await header.locator('svg[data-icon="sparkles"]').count()) === 0 &&
      ((await header.textContent()) ?? "").trim() === "Agents",
    JSON.stringify(await header.textContent()),
  );
  check(
    "and the rows of every agent follow it",
    (await fixGroup.locator("[data-agent-id]").count()) === 4,
  );

  await p.mouse.move(0, 0);
  await p.waitForTimeout(200);
  const styles = await p.evaluate(() => {
    const header = document.querySelector(
      '[data-agent-group="/home/dev/agent-ade-worktrees/fix-pty"] > button',
    );
    const title = header.querySelector("span");
    const repoName = document.querySelector('[data-project-id="r1"] span');
    const h = getComputedStyle(header);
    const t = getComputedStyle(title);
    const r = getComputedStyle(repoName);
    return {
      headerBg: h.backgroundColor,
      titleWeight: t.fontWeight,
      repoWeight: r.fontWeight,
      titleColor: t.color,
      repoColor: r.color,
    };
  });
  check(
    "the group header has no colour of its own until hovered",
    styles.headerBg === "rgba(0, 0, 0, 0)",
    JSON.stringify(styles),
  );
  check(
    "Agents has the same weight and colour as the repo name",
    styles.titleWeight === styles.repoWeight &&
      styles.titleColor === styles.repoColor,
    JSON.stringify(styles),
  );
  await header.hover();
  await p.waitForTimeout(200);
  check(
    "hovering the header is the only thing that changes it",
    (await header.evaluate((el) => getComputedStyle(el).backgroundColor)) !==
      "rgba(0, 0, 0, 0)",
  );
});

await session("?mock=empty", 1000, 700, async (p) => {
  await p.getByRole("button", { name: "Add project" }).last().click();
  await p.waitForTimeout(300);
  const chooser = p.getByRole("dialog", { name: "Add project" });
  check(
    "Add project offers browsing an existing project or creating a new one",
    (await chooser.getByRole("button", { name: /Browse project/ }).count()) ===
      1 &&
      (await chooser
        .getByRole("button", { name: /Create new project/ })
        .count()) === 1,
  );

  await chooser.getByRole("button", { name: /Browse project/ }).click();
  await p.waitForTimeout(500);
  check(
    "browsing closes the dialog and adds the chosen repository",
    (await p.getByRole("dialog").count()) === 0 &&
      (await p.getByText("web-app").count()) >= 1,
  );
  check(
    "its main worktree is selected and offers the launcher",
    (await p.getByText("Nothing is running in main").count()) === 1,
  );
});

await session("?mock=empty", 1000, 700, async (p) => {
  await p.getByRole("button", { name: "Add project" }).last().click();
  await p.getByRole("button", { name: /Create new project/ }).click();
  await p.waitForTimeout(300);
  const create = p.getByRole("button", { name: "Create project" });
  check(
    "creating needs a name and a location before it is enabled",
    (await create.isDisabled()) &&
      (await p.getByText("No folder chosen").count()) === 1,
  );

  await p.getByRole("button", { name: "Choose folder…" }).click();
  await p.waitForTimeout(300);
  await p.getByLabel("Project name").fill("notes");
  check(
    "the form says where the project folder is created",
    (await p.getByText("/home/dev/web-app", { exact: true }).count()) === 1 &&
      (await create.isEnabled()),
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the create form open",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByLabel("Project name").fill("a/b");
  await create.click();
  await p.waitForTimeout(300);
  check(
    "a refused name keeps the form open with the reason and the typed name",
    (await p.getByRole("alert").innerText()).includes("invalid name") &&
      (await p.getByRole("dialog", { name: "Create new project" }).count()) ===
        1 &&
      (await p.getByLabel("Project name").inputValue()) === "a/b",
  );

  await p.getByLabel("Project name").fill("notes");
  await create.click();
  await p.waitForTimeout(600);
  check(
    "a valid name creates the project and adds it to the sidebar",
    (await p.getByRole("dialog").count()) === 0 &&
      (await p.getByText("notes", { exact: true }).count()) >= 1 &&
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
