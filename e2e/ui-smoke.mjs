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
    (await p.getByText("lynx-ide / feat/auth").count()) === 1,
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
  check(
    "the scrollbar gutter is empty: the xterm overview ruler draws no edge line",
    await p.evaluate(() => {
      const ruler = document.querySelector(".xterm-decoration-overview-ruler");
      return !ruler || getComputedStyle(ruler).display === "none";
    }),
  );

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

// Launch arguments are offered once an agent is on, and the launch mode applies to every agent.
await session("?mock=full", 1200, 900, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(600);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await p.waitForTimeout(300);

  await p.getByRole("switch", { name: "Use Claude Code in Lynx" }).click();
  await p.waitForTimeout(300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Turn on", exact: true })
    .click();
  await p.waitForTimeout(600);

  const argumentsSwitch = p.getByRole("switch", {
    name: "Launch arguments for Claude Code",
  });
  const argumentsBox = p.getByRole("textbox", {
    name: "Arguments for Claude Code",
  });
  check(
    "an agent that is on offers its launch arguments, off, with no text box",
    (await argumentsSwitch.getAttribute("aria-checked")) === "false" &&
      (await argumentsBox.count()) === 0,
  );

  await argumentsSwitch.click();
  await p.waitForTimeout(300);
  check(
    "turning the arguments on shows the text box",
    (await argumentsBox.count()) === 1,
  );
  await argumentsBox.fill("--model opus");
  await argumentsBox.press("Enter");
  await p.waitForTimeout(400);
  check(
    "the arguments typed in the box are kept",
    (await argumentsBox.inputValue()) === "--model opus",
  );

  await p.getByRole("combobox", { name: "Launch mode" }).click();
  await p.getByRole("option", { name: "YOLO", exact: true }).click();
  await p.waitForTimeout(300);
  const confirm = p.getByRole("alertdialog");
  check(
    "YOLO asks before it is turned on",
    (await confirm.count()) === 1 &&
      ((await confirm.textContent()) ?? "").includes("YOLO mode"),
  );
  await confirm.getByRole("button", { name: "Turn on YOLO" }).click();
  await p.waitForTimeout(500);
  check(
    "in YOLO mode the agent says which flag it starts with",
    (await p
      .getByText("YOLO mode adds --dangerously-skip-permissions before them.")
      .count()) === 1,
  );

  await p.keyboard.press("Escape");
  await p.waitForTimeout(300);
  // Several rows have a "New terminal" button; the one in the tab bar sits at the top of the main area.
  const launchers = p.getByRole("button", { name: "New terminal" });
  let launcher = null;
  for (let index = 0; index < (await launchers.count()); index += 1) {
    const box = await launchers.nth(index).boundingBox();
    if (box && box.y < 60 && box.x > 300) {
      launcher = launchers.nth(index);
      break;
    }
  }
  check("the tab bar has a new terminal button", launcher !== null);
  await launcher.click();
  await p.getByRole("menuitem", { name: "Claude Code" }).click();
  await p.waitForTimeout(500);
  check(
    "a Claude Code terminal opens from the menu",
    (await p.getByRole("tab", { name: /claude/i }).count()) >= 1,
  );
});

// The agents found on this computer get a switch. Turning one on rewrites its settings, so it is asked first.
await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(600);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await p.waitForTimeout(300);

  const onComputer = p.getByRole("region", { name: "On this computer" });
  const elsewhere = p.getByRole("region", { name: "Other supported agents" });
  const claudeSwitch = p.getByRole("switch", {
    name: "Use Claude Code in Lynx",
  });
  check(
    "Claude Code is found on this computer, with its switch off",
    (await onComputer.getByText("Claude Code").count()) === 1 &&
      (await claudeSwitch.getAttribute("aria-checked")) === "false",
  );
  check(
    "an agent not found here is listed for information, with no switch",
    (await elsewhere.getByText("OpenCode").count()) === 1 &&
      (await elsewhere.getByText("Not found on this computer").count()) > 0 &&
      (await p
        .getByRole("switch", { name: "Use OpenCode in Lynx" })
        .count()) === 0,
  );

  check(
    "an agent that is off offers no launch arguments",
    (await p
      .getByRole("switch", { name: "Launch arguments for Claude Code" })
      .count()) === 0,
  );

  await claudeSwitch.click();
  await p.waitForTimeout(300);
  const confirm = p.getByRole("alertdialog");
  const confirmText = (await confirm.textContent()) ?? "";
  check(
    "turning it on asks first and names the settings file and the events",
    (await confirm.count()) === 1 &&
      confirmText.includes("/home/dev/.claude/settings.json") &&
      confirmText.includes("SessionStart"),
    confirmText,
  );

  await confirm.getByRole("button", { name: "Cancel" }).click();
  await p.waitForTimeout(300);
  check(
    "cancelling leaves the switch off",
    (await p.getByRole("alertdialog").count()) === 0 &&
      (await claudeSwitch.getAttribute("aria-checked")) === "false",
  );

  await claudeSwitch.click();
  await p.waitForTimeout(300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Turn on", exact: true })
    .click();
  await p.waitForTimeout(400);
  check(
    "confirming turns Claude Code on",
    (await p.getByRole("alertdialog").count()) === 0 &&
      (await claudeSwitch.getAttribute("aria-checked")) === "true",
  );

  const axeAgents = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations on the Agents section",
    axeAgents.length === 0,
    axeAgents.join(" || "),
  );
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
        (await item(/^src(,|$)/).count()) === 1 &&
        (await item(/^README\.md(,|$)/).count()) === 1,
    );
    check(
      "git's own folder is not listed",
      (await item(/^\.git$/).count()) === 0,
    );
    check(
      "a folder starts collapsed",
      (await item(/^src(,|$)/).getAttribute("aria-expanded")) === "false",
    );

    await item(/^src(,|$)/).click();
    await p.waitForTimeout(400);
    check(
      "opening a folder loads and shows its children one level deeper",
      (await item(/^src(,|$)/).getAttribute("aria-expanded")) === "true" &&
        (await item(/^App\.tsx(,|$)/).count()) === 1 &&
        (await item(/^App\.tsx(,|$)/).getAttribute("aria-level")) === "2",
    );
    check(
      "a changed file shows its status letter and says the change in its name",
      (await item(/^App\.tsx, modified$/).count()) === 1 &&
        (await item(/^App\.tsx, modified$/)
          .locator("span[aria-hidden]")
          .last()
          .innerText()) === "M",
    );
    await item(/^src(,|$)/).click();
    await p.waitForTimeout(200);
    check(
      "clicking it again closes it",
      (await item(/^App\.tsx(,|$)/).count()) === 0,
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
    await item(/^docs(,|$)/).click();
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
      (await item(/^docs(,|$)/).getAttribute("aria-expanded")) === "false",
    );
    await p.keyboard.press("ArrowRight");
    await p.waitForTimeout(300);
    check(
      "ArrowRight opens it again",
      (await item(/^docs(,|$)/).getAttribute("aria-expanded")) === "true",
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
    await item(/^README\.md(,|$)/).hover();
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

    await item(/^docs(,|$)/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Copy relative path" }).click();
    await p.waitForTimeout(300);
    check(
      "Copy relative path puts it on the clipboard",
      (await p.evaluate(() => navigator.clipboard.readText())) === "docs",
    );
    await item(/^docs(,|$)/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Copy path", exact: true }).click();
    await p.waitForTimeout(300);
    check(
      "Copy path gives the absolute path",
      (await p.evaluate(() => navigator.clipboard.readText())).endsWith(
        "/lynx-ide-worktrees/feat-auth/docs",
      ),
      await p.evaluate(() => navigator.clipboard.readText()),
    );
    await item(/^README\.md(,|$)/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Open containing folder" }).click();
    await p.waitForTimeout(200);
    check(
      "Open containing folder asks the backend with the worktree and the item",
      (await p.evaluate(() => window.__lynxMock.probe.revealed))?.endsWith(
        "::README.md",
      ) ?? false,
    );

    // The menu is reachable without a mouse.
    await item(/^README\.md(,|$)/).focus();
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

await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  const item = (name) => p.getByRole("treeitem", { name });
  const fileTabs = p.getByRole("tablist", { name: "Open files" });

  await item(/^src(,|$)/).click();
  await p.waitForTimeout(300);
  await item(/^App\.tsx(,|$)/).click();
  check(
    "one click selects a file without opening it",
    (await fileTabs.count()) === 0 &&
      (await p.locator(".cm-editor").count()) === 0,
  );
  await item(/^App\.tsx(,|$)/).dblclick();
  await p.locator(".cm-editor").waitFor({ timeout: 5000 });
  check(
    "double click opens the file in the editor under the terminal tabs",
    (await fileTabs.getByRole("tab", { name: /App\.tsx/ }).count()) === 1 &&
      (await p.locator(".cm-content").innerText()).includes(
        "export function App",
      ),
  );
  const tabsBox = await p
    .getByRole("tablist", { name: "Terminals" })
    .boundingBox();
  const filesBox = await fileTabs.boundingBox();
  check(
    "the file strip sits below the terminal strip",
    !!tabsBox && !!filesBox && filesBox.y >= tabsBox.y + tabsBox.height - 1,
    JSON.stringify({ tabsBox, filesBox }),
  );
  check(
    "no terminal tab is selected while a file is shown",
    (await p
      .getByRole("tablist", { name: "Terminals" })
      .getByRole("tab", { selected: true })
      .count()) === 0,
  );
  check(
    "the file tab uses colours from the highlighting tokens",
    await p.evaluate(() => {
      const probe = document.querySelector(".cm-line span");
      return !!probe && getComputedStyle(probe).color !== "";
    }),
  );

  await p.locator(".cm-content").click();
  await p.keyboard.press("End");
  await p.keyboard.type(" // edited");
  check(
    "typing marks the file as unsaved",
    (await fileTabs.getByText("unsaved changes").count()) === 1,
  );
  await p.keyboard.press("Control+s");
  await p.waitForTimeout(300);
  check(
    "Ctrl+S saves it",
    (await fileTabs.getByText("unsaved changes").count()) === 0,
  );

  // Another program changes the file; the next save must not overwrite it silently.
  await p.evaluate(() => {
    window.__lynxMock.touchFile(
      "/home/dev/lynx-ide-worktrees/feat-auth",
      "src/App.tsx",
      "changed elsewhere\n",
    );
  });
  await p.keyboard.type("x");
  await p.keyboard.press("Control+s");
  await p.waitForTimeout(300);
  check(
    "a file changed on disk shows the conflict instead of saving",
    (await p
      .getByRole("alert")
      .filter({ hasText: "changed on disk" })
      .count()) === 1,
  );
  await p.getByRole("button", { name: "Reload from disk" }).click();
  await p.waitForTimeout(300);
  check(
    "reloading takes the text from disk",
    (await p.locator(".cm-content").innerText()).includes("changed elsewhere"),
  );

  const axeEditor = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations with the editor open",
    axeEditor.length === 0,
    axeEditor.join(" || "),
  );

  await item(/^package\.json(,|$)/).click();
  await p.keyboard.press("Enter");
  await p.waitForTimeout(400);
  check(
    "Enter opens the selected file from the keyboard",
    (await fileTabs.getByRole("tab").count()) === 2,
  );

  await p
    .getByRole("tablist", { name: "Terminals" })
    .getByRole("tab")
    .first()
    .click();
  await p.waitForTimeout(300);
  check(
    "choosing a terminal tab brings the terminal back, and the files stay open",
    (await p.locator(".cm-editor").count()) === 0 &&
      (await fileTabs.getByRole("tab").count()) === 2,
  );

  await item(/^logo\.png$/).dblclick();
  await p.waitForTimeout(400);
  check(
    "a file that is not text says so instead of opening",
    (await p.getByRole("alert").filter({ hasText: "not text" }).count()) === 1,
  );

  await fileTabs.getByRole("tab", { name: /App\.tsx/ }).click();
  await p.locator(".cm-content").click();
  await p.keyboard.type("y");
  await fileTabs.getByRole("tab", { name: /App\.tsx/ }).press("Delete");
  await p.waitForTimeout(300);
  check(
    "closing a file with unsaved changes asks first",
    (await p.getByRole("alertdialog").count()) === 1,
  );
  await p.getByRole("button", { name: "Close without saving" }).click();
  await p.waitForTimeout(300);
  check(
    "confirming closes the file",
    (await fileTabs.getByRole("tab", { name: /App\.tsx/ }).count()) === 0,
  );
});

await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(400);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(500);
  const list = p.getByRole("group", {
    name: "Changed files in the selected worktree",
  });
  const row = (name) => list.getByRole("treeitem", { name });
  const fileTabs = p.getByRole("tablist", { name: "Open files" });
  const group = (name) => p.getByRole("tree", { name });

  check(
    "the Changes tab lists the sections in Git's order",
    (await list.locator("h3").allInnerTexts())
      .map((t) => t.replace(/\s+/g, " ").trim())
      .join("|") === "STAGED CHANGES 3|CHANGES 3|UNTRACKED FILES 1",
    (await list.locator("h3").allInnerTexts()).join("|"),
  );
  check(
    "a file with staged and unstaged changes is in both sections",
    (await group(/^Staged changes/)
      .getByRole("treeitem", { name: /^README\.md/ })
      .count()) === 1 &&
      (await group(/^Changes/)
        .getByRole("treeitem", { name: /^README\.md/ })
        .count()) === 1,
  );
  check(
    "the row says what happened to the file, not only by colour",
    (await row(/^Legacy\.tsx, deleted$/).count()) === 1 &&
      (await row(/^setup\.md, untracked$/).count()) === 1,
  );

  const changesToggle = list.getByRole("button", { name: /^Changes/ });
  await changesToggle.click();
  check(
    "a foldable section hides its files and keeps its count",
    (await changesToggle.getAttribute("aria-expanded")) === "false" &&
      (await group(/^Changes/).count()) === 0 &&
      (await changesToggle.innerText()).includes("3"),
  );
  await changesToggle.click();
  check(
    "clicking the section again shows its files",
    (await changesToggle.getAttribute("aria-expanded")) === "true" &&
      (await row(/^App\.tsx, modified$/).count()) === 1,
  );
  const untrackedToggle = list.getByRole("button", {
    name: /^Untracked files/,
  });
  await untrackedToggle.click();
  check(
    "untracked files fold the same way",
    (await untrackedToggle.getAttribute("aria-expanded")) === "false" &&
      (await row(/^setup\.md, untracked$/).count()) === 0,
  );
  await untrackedToggle.click();

  await row(/^App\.tsx, modified$/).click();
  await p.locator(".cm-editor").waitFor({ timeout: 5000 });
  await p.waitForTimeout(300);
  const appTab = fileTabs.getByRole("tab", { name: /App\.tsx/ });
  check(
    "one click opens the file as a temporary tab in diff view",
    (await appTab.count()) === 1 &&
      (await appTab.locator("span.italic").count()) === 1 &&
      (await p.locator(".cm-changedLine, .cm-deletedChunk").count()) > 0,
  );
  check(
    "the toolbar shows the file, its status and Diff selected",
    (await p
      .getByRole("toolbar", { name: "File view" })
      .getByRole("button", { name: "Diff", pressed: true })
      .count()) === 1 &&
      (await p.getByRole("toolbar").getByText("modified").count()) >= 1,
  );

  await row(/^Button\.tsx, renamed$/).click();
  await p.waitForTimeout(500);
  check(
    "another single click replaces the temporary tab",
    (await fileTabs.getByRole("tab").count()) === 1 &&
      (await fileTabs.getByRole("tab", { name: /Button\.tsx/ }).count()) === 1,
  );

  await row(/^package\.json, modified$/).dblclick();
  await p.waitForTimeout(500);
  const pkgTab = fileTabs.getByRole("tab", { name: /package\.json/ });
  check(
    "double click opens it as a permanent tab",
    (await pkgTab.locator("span.italic").count()) === 0,
  );
  await row(/^App\.tsx, modified$/).click();
  await p.waitForTimeout(500);
  check(
    "a permanent tab survives the next temporary one",
    (await fileTabs.getByRole("tab").count()) === 2 &&
      (await pkgTab.count()) === 1 &&
      (await fileTabs.getByRole("tab", { name: /Button\.tsx/ }).count()) === 0,
  );

  const toolbar = p.getByRole("toolbar", { name: "File view" });
  await toolbar.getByRole("button", { name: "Split" }).click();
  await p.waitForTimeout(400);
  check(
    "Split shows the original and the new text side by side",
    (await p.locator(".cm-merge-a").count()) === 1 &&
      (await p.locator(".cm-merge-b").count()) === 1,
  );
  await toolbar.getByRole("button", { name: "Unified" }).click();
  await toolbar.getByRole("button", { name: "Edit" }).click();
  await p.waitForTimeout(400);
  check(
    "Edit switches the same tab to the editor",
    (await p.locator(".cm-content[contenteditable=true]").count()) === 1 &&
      (await p.locator(".cm-mergeView").count()) === 0,
  );
  await p.locator(".cm-content").click();
  await p.keyboard.type("// local");
  await toolbar.getByRole("button", { name: "Diff" }).click();
  await p.waitForTimeout(400);
  check(
    "the diff includes text that is not saved yet",
    (await p.locator(".cm-line", { hasText: "// local" }).count()) >= 1,
  );

  await row(/^Legacy\.tsx, deleted$/).click();
  await p.waitForTimeout(500);
  check(
    "a deleted file opens as a diff, and Edit is off because there is nothing to edit",
    (await p.locator(".cm-deletedLine, .cm-deletedChunk").count()) >= 0 &&
      (await toolbar.getByRole("button", { name: "Edit" }).isDisabled()),
  );

  await p.getByRole("tab", { name: "Files", exact: true }).click();
  await p.waitForTimeout(300);
  await p.getByRole("treeitem", { name: /^README\.md(,|$)/ }).dblclick();
  await p.waitForTimeout(400);
  check(
    "a file opened from Files starts in the editor, and Diff is on because it changed",
    (await toolbar
      .getByRole("button", { name: "Edit", pressed: true })
      .count()) === 1 &&
      (await toolbar.getByRole("button", { name: "Diff" }).isEnabled()),
  );
  await p.getByRole("treeitem", { name: /^LICENSE$/ }).dblclick();
  await p.waitForTimeout(400);
  check(
    "Diff is off for a file that did not change",
    await toolbar.getByRole("button", { name: "Diff" }).isDisabled(),
  );

  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(300);
  await row(/^App\.tsx, modified$/).click();
  await p.waitForTimeout(500);
  const axeDiff = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) =>
        `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)} ${v.nodes[0].any[0]?.message ?? ""}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations with the changes list and a diff open",
    axeDiff.length === 0,
    axeDiff.join(" || "),
  );
});

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
    (await p.getByRole("treeitem", { name: /^README\.md(,|$)/ }).count()) === 1,
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
  const featPath = "/home/dev/lynx-ide-worktrees/feat-auth";
  const fixPath = "/home/dev/lynx-ide-worktrees/fix-pty";
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
    (await agent(/^claude, idle$/).count()) === 1,
  );
  check(
    "plain shells are not listed as agents",
    (await agentRows.count()) === 1,
  );
  check(
    "an agent row says how long ago it started",
    ((await agent(/^claude, idle$/).textContent()) ?? "").includes("5m"),
  );
  check(
    "another worktree's agents stay collapsed until opened",
    (await agent(/^codex/).count()) === 0,
  );

  await p.getByRole("tab", { name: /zsh/ }).click();
  await agent(/^claude, idle$/).click();
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
    (await agent(/^claude, idle$/).getAttribute("aria-current")) === "true",
  );

  const claudeCurrent = (value) =>
    p
      .waitForFunction(
        (expected) =>
          document
            .querySelector("[data-agent-id]")
            ?.getAttribute("aria-current") === expected,
        value,
        { timeout: 5000 },
      )
      .then(
        () => true,
        () => false,
      );
  await p.getByRole("button", { name: /^fix\/pty/ }).click();
  check(
    "an agent of a worktree that is not selected is not marked current",
    await claudeCurrent(null),
  );
  await agent(/^claude, idle$/).click();
  check(
    "clicking it selects its worktree and brings its tab forward",
    (await claudeCurrent("true")) &&
      (await p.getByRole("tab", { name: /claude/, selected: true }).count()) ===
        1,
  );

  // The card around a worktree and its agents is one click target, not just the name line.
  await p.getByRole("button", { name: /^fix\/pty/ }).click();
  const featCard = groupOf(featPath).locator("xpath=../..");
  const featCardBox = await featCard.boundingBox();
  await featCard.click({
    position: { x: 10, y: (featCardBox?.height ?? 4) - 2 },
  });
  check(
    "clicking the card outside its buttons selects the worktree",
    await p
      .waitForFunction(
        () =>
          [...document.querySelectorAll("button[aria-current]")].some((b) =>
            /^feat\/auth/.test(b.textContent ?? ""),
          ),
        null,
        { timeout: 5000 },
      )
      .then(
        () => true,
        () => false,
      ),
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
    (await agent(/^claude, idle$/).count()) === 1,
  );
  check(
    "the newly selected worktree's agents stay collapsed until opened",
    (await agent(/^codex, needs you$/).count()) === 0,
  );
  await toggleGroup(fixPath).click();
  await p.waitForTimeout(300);
  check(
    "each row shows its own age",
    ((await agent(/^codex, needs you$/).textContent()) ?? "").includes("3h"),
  );

  await toggleGroup(refactorPath).click();
  await p.waitForTimeout(300);
  check(
    "a finished agent is shown as exited, with its age",
    (await agent(/^claude, exited with code 0$/).count()) === 1 &&
      ((await agent(/^claude, exited/).textContent()) ?? "").includes("2d"),
  );

  await agent(/^claude, idle$/).hover();
  await groupOf(featPath).getByRole("button", { name: "Close claude" }).click();
  await p.waitForTimeout(400);
  check(
    "closing from the row removes the agent and its tab",
    (await agent(/^claude, idle$/).count()) === 0 &&
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
      '[data-agent-group="/home/dev/lynx-ide-worktrees/feat-auth"] > button',
    )
    .click();
  await p.waitForTimeout(300);
  const agentRow = p.getByRole("button", { name: /^claude, idle$/ });
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
  // The dialog's code loads on first open, so wait for its text rather than a fixed time.
  await p
    .getByText("Its folders and projects move up one level", { exact: false })
    .first()
    .waitFor({ timeout: 5000 })
    .catch(() => undefined);
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

  await p.getByRole("button", { name: "Collapse lynx-ide" }).click();
  await p.waitForTimeout(200);
  const spoken = await spokenOf(repoHeader);
  check(
    "a collapsed repo says what it hides, including the agents running in it and those that need the user",
    spoken === "3 worktrees, 2 agents running, 1 agent needs you",
    spoken ?? "null",
  );
  const shownInRow =
    (await repoHeader.locator('span[aria-hidden="true"]').textContent()) ?? "";
  check(
    "the row shows the worktree count, the running agents and the ones that need the user at its right edge",
    shownInRow.includes("3 worktrees") && shownInRow.endsWith("1"),
    shownInRow,
  );
  await p.getByRole("button", { name: "Expand lynx-ide" }).click();
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
    '[data-agent-group="/home/dev/lynx-ide-worktrees/fix-pty"]',
  );
  const header = fixGroup.locator(":scope > button");
  const badges = header.locator('svg[data-icon="agent"]');

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
    (await header.locator('svg[data-icon="agent"]').count()) === 0 &&
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
      '[data-agent-group="/home/dev/lynx-ide-worktrees/fix-pty"] > button',
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
    (await p.getByText("lynx-ide / feat/auth").count()) === 1,
  );
});

// Thousands of branches must not freeze the New worktree dialog: the base picker
// renders a bounded list, and the search still reaches the rest.
await session("?mock=full", 1200, 760, async (p) => {
  await p.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => {
      if (cmd !== "repo_refs") return original(cmd, args, options);
      const refs = ["main"];
      for (let i = 0; i < 4000; i++) {
        refs.push(`feature/topic-${String(i).padStart(4, "0")}`);
      }
      return Promise.resolve({ defaultBase: "origin/main", refs: refs.sort() });
    };
  });
  await p.getByRole("button", { name: "New worktree" }).first().click();
  const base = p.locator("#worktree-base");
  await base.and(p.locator(":not([disabled])")).waitFor();
  await base.click();
  await p.getByRole("listbox").waitFor();
  const rows = await p.getByRole("option").count();
  check(
    "a repo with 4000 branches shows a bounded list",
    rows > 0 && rows <= 100,
    String(rows),
  );
  check(
    "the picker says how many branches are left out",
    (await p.getByText("Showing 100 of 4002").count()) === 1,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the base picker open",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByPlaceholder("Search branches…").fill("topic-0042");
  await p
    .getByRole("option", { name: "feature/topic-0042", exact: true })
    .waitFor();
  check(
    "searching narrows the list to the match",
    (await p.getByRole("option").count()) === 1,
  );
  await p
    .getByRole("option", { name: "feature/topic-0042", exact: true })
    .click();
  await p.getByRole("listbox").waitFor({ state: "detached" });
  check(
    "picking a searched branch makes it the base",
    (await base.textContent()).includes("feature/topic-0042"),
    await base.textContent(),
  );
});

// The status bar is a low strip under the whole window, and says no language server runs yet.
await session("?mock=full", 1200, 760, async (p) => {
  const bar = p.getByRole("contentinfo", { name: "Status bar" });
  check(
    "the status bar is a footer landmark named Status bar",
    (await bar.count()) === 1,
  );
  check(
    "the status bar says no language server runs",
    ((await bar.textContent()) ?? "").includes("No language server running"),
  );
  const box = await bar.boundingBox();
  const viewport = p.viewportSize();
  check(
    "the status bar is low and sits at the bottom of the window",
    box !== null &&
      box.height <= 28 &&
      Math.abs(box.y + box.height - viewport.height) < 2,
    JSON.stringify(box),
  );

  // The primary bar runs the full height on the left. The status bar starts at its right edge and
  // stops at the top of the status bar, so the project panel sits above it.
  const rail = await p
    .getByRole("navigation", { name: "Primary navigation" })
    .boundingBox();
  const projectPanel = await p
    .getByRole("complementary", { name: "Projects" })
    .boundingBox();
  check(
    "the status bar starts at the edge of the primary bar and the project panel stays above it",
    rail !== null &&
      projectPanel !== null &&
      box !== null &&
      Math.abs(box.x - (rail.x + rail.width)) < 2 &&
      Math.abs(rail.y + rail.height - viewport.height) < 2 &&
      projectPanel.y + projectPanel.height <= box.y + 1,
    `${JSON.stringify(rail)} ${JSON.stringify(projectPanel)} ${JSON.stringify(box)}`,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the status bar in view",
    axe.length === 0,
    axe.join(" || "),
  );
});

// Stage, commit and unstage from the Changes tab. The mock moves the files, like git would.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(500);
  const list = p.getByRole("group", {
    name: "Changed files in the selected worktree",
  });
  const group = (name) => p.getByRole("tree", { name });
  const summary = p.getByRole("textbox", { name: "Commit summary" });
  const description = p.getByRole("textbox", { name: "Commit description" });
  const commit = p.getByRole("button", { name: "Commit", exact: true });

  await list.getByRole("button", { name: "Stage App.tsx" }).click();
  await p.waitForTimeout(400);
  check(
    "the stage button moves a changed file into the staged section",
    (await group(/^Staged changes/)
      .getByRole("treeitem", { name: /^App\.tsx/ })
      .count()) === 1 &&
      (await group(/^Changes/)
        .getByRole("treeitem", { name: /^App\.tsx/ })
        .count()) === 0,
  );
  check(
    "Commit stays off until the summary has text",
    await commit.isDisabled(),
  );

  await list.getByRole("treeitem", { name: /^setup\.md, untracked$/ }).focus();
  await p.keyboard.press("s");
  await p.waitForTimeout(400);
  check(
    "S on a focused row stages it",
    (await group(/^Staged changes/)
      .getByRole("treeitem", { name: /^setup\.md/ })
      .count()) === 1,
  );

  await list.getByRole("button", { name: "Unstage setup.md" }).click();
  await p.waitForTimeout(400);
  check(
    "unstage moves the file back to untracked, and the working tree keeps it",
    (await group(/^Untracked files/)
      .getByRole("treeitem", { name: /^setup\.md/ })
      .count()) === 1 &&
      (await group(/^Staged changes/)
        .getByRole("treeitem", { name: /^setup\.md/ })
        .count()) === 0,
  );
  await list.getByRole("treeitem", { name: /^setup\.md, untracked$/ }).focus();
  await p.keyboard.press("s");
  await p.waitForTimeout(400);

  await p.getByRole("button", { name: "Generate message with AI" }).click();
  await p.waitForTimeout(500);
  check(
    "Generate fills the message and says which agent wrote it",
    (await summary.inputValue()) === "Update the Changes panel" &&
      (await description.inputValue()) === "Describes the staged files." &&
      (await p
        .getByRole("status")
        .filter({ hasText: "Drafted with Claude Code" })
        .count()) === 1,
  );

  await summary.fill("Fix the button label");
  await description.fill("The label was wrong.");
  check(
    "Commit turns on with a summary and staged files",
    await commit.isEnabled(),
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the commit box and staged files",
    axe.length === 0,
    axe.join(" || "),
  );

  await commit.click();
  await p.waitForTimeout(600);
  check(
    "a commit empties the index and clears the message",
    (await group(/^Staged changes/).count()) === 0 &&
      (await summary.inputValue()) === "" &&
      (await description.inputValue()) === "",
  );

  check(
    "a committed file that stays changed is still listed",
    (await group(/^Changes/)
      .getByRole("treeitem", { name: /^README\.md/ })
      .count()) === 1,
  );
});

// The Git tab: which agent writes commit messages, the style it follows, and the instructions box.
await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(400);
  await p.getByRole("tab", { name: "Git", exact: true }).click();
  await p.waitForTimeout(300);

  check(
    "the Git tab starts on the automatic agent and Conventional Commits",
    ((await p.locator("#git-commit-agent").textContent()) ?? "").includes(
      "Automatic",
    ) &&
      ((await p.locator("#git-commit-style").textContent()) ?? "").includes(
        "Conventional Commits",
      ),
  );
  check(
    "the instructions box only shows for the Custom style",
    (await p.locator("#git-commit-instructions").count()) === 0,
  );

  await p.locator("#git-commit-style").click();
  await p.getByRole("option", { name: "Custom", exact: true }).click();
  await p.waitForTimeout(400);
  const instructions = p.locator("#git-commit-instructions");
  check(
    "choosing Custom shows the instructions box",
    (await instructions.count()) === 1,
  );
  await instructions.fill("Write the subject in Portuguese.");
  await instructions.blur();
  await p.waitForTimeout(400);
  check(
    "the instructions save when the box loses focus",
    (await instructions.inputValue()) === "Write the subject in Portuguese.",
  );

  await p.locator("#git-commit-agent").click();
  await p.getByRole("option", { name: "Codex", exact: true }).click();
  await p.waitForTimeout(400);
  check(
    "the agent choice saves",
    ((await p.locator("#git-commit-agent").textContent()) ?? "").includes(
      "Codex",
    ),
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Git settings",
    axe.length === 0,
    axe.join(" || "),
  );
});

// The section buttons stage every file of their section at once.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(500);
  const list = p.getByRole("group", {
    name: "Changed files in the selected worktree",
  });
  const tree = (name) => p.getByRole("tree", { name });

  check(
    "the Changes and untracked sections have a stage-all button",
    (await list
      .getByRole("button", { name: "Stage all changes", exact: true })
      .count()) === 1 &&
      (await list
        .getByRole("button", { name: "Stage all untracked files", exact: true })
        .count()) === 1,
  );

  await list
    .getByRole("button", { name: "Stage all untracked files", exact: true })
    .click();
  await p.waitForTimeout(400);
  check(
    "stage all untracked files moves every untracked file into the staged section",
    (await tree(/^Staged changes/)
      .getByRole("treeitem", { name: /^setup\.md/ })
      .count()) === 1 && (await tree(/^Untracked files/).count()) === 0,
  );

  await list
    .getByRole("button", { name: "Stage all changes", exact: true })
    .click();
  await p.waitForTimeout(400);
  check(
    "stage all changes moves every unstaged file, deletions included, into the index",
    (await tree(/^Staged changes/)
      .getByRole("treeitem", { name: /^Legacy\.tsx/ })
      .count()) === 1 &&
      (await tree(/^Staged changes/)
        .getByRole("treeitem", { name: /^App\.tsx/ })
        .count()) === 1 &&
      (await tree(/^Changes/).count()) === 0,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the stage-all buttons",
    axe.length === 0,
    axe.join(" || "),
  );
});

// The Pull request tab: the branch's PR, its checks, and the link to GitHub.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(600);

  check(
    "the tab shows the pull request of the branch, with its number and title",
    (await p.getByRole("heading", { name: /^#42 fix\(auth\)/ }).count()) === 1,
  );
  check(
    "the checks are counted by state",
    (await p.getByText("1 passed · 1 failed · 1 pending").count()) === 1,
  );
  const checks = p.getByRole("region", { name: "Checks" });
  check(
    "a failing check comes first in the list",
    ((await checks.getByRole("listitem").first().textContent()) ?? "").includes(
      "CI / test",
    ),
  );
  check(
    "the state of each check is written out, not only drawn",
    (await p.getByText("Skipped", { exact: true }).count()) === 1,
  );

  await p.getByRole("button", { name: "Open on GitHub" }).click();
  await p.waitForTimeout(300);
  check(
    "Open on GitHub asks the backend for the pull request link",
    (await p.getByRole("alert").count()) === 0,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the pull request shown",
    axe.length === 0,
    axe.join(" || "),
  );
});

// A repository with no GitHub remote: the panel offers to create one, and the dialog creates it.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^fix\/pty/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(600);

  const createButton = p.getByRole("button", {
    name: "Create remote",
    exact: true,
  });
  check(
    "without a GitHub remote, Create remote sits beside Retry",
    (await createButton.count()) === 1 &&
      (await p.getByRole("button", { name: "Retry", exact: true }).count()) ===
        1,
  );

  await createButton.click();
  await p.waitForTimeout(500);
  const dialog = p.getByRole("dialog");
  check(
    "the dialog opens with the signed-in account, the folder name and private selected",
    (
      await dialog.getByRole("combobox", { name: "Account" }).innerText()
    ).includes("zdearo") &&
      (await dialog
        .getByRole("textbox", { name: "Repository name" })
        .inputValue()) === "lynx-ide" &&
      (await dialog.getByRole("radio", { name: /Private/ }).isChecked()) ===
        true &&
      (await dialog.getByRole("switch").isChecked()) === true,
  );

  const name = dialog.getByRole("textbox", { name: "Repository name" });
  const confirm = dialog.getByRole("button", {
    name: "Create repository",
    exact: true,
  });
  await name.fill("has space");
  check(
    "a name GitHub would refuse keeps Create repository off",
    await confirm.isDisabled(),
  );
  await name.fill("lynx-ide-remote");
  check("a valid name turns Create repository on", await confirm.isEnabled());

  // The button fades in when it turns on; axe would measure the colour mid-transition.
  await p.waitForTimeout(400);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations in the create remote dialog",
    axe.length === 0,
    axe.join(" || "),
  );

  await confirm.click();
  await p.waitForTimeout(700);
  check(
    "confirming creates the remote and the pull request panel reads the branch again",
    (await p.getByRole("dialog").count()) === 0 &&
      (await p
        .getByRole("heading", { name: "Open a pull request" })
        .count()) === 1,
  );
});

// After a commit, the button under the file list becomes Push while the remote lacks commits.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(600);

  const commit = p.getByRole("button", { name: "Commit", exact: true });
  const push = p.getByRole("button", { name: "Push", exact: true });
  check(
    "with files staged and nothing to push, the button is Commit",
    (await commit.count()) === 1 && (await push.count()) === 0,
  );

  await p
    .getByRole("textbox", { name: "Commit summary" })
    .fill("Fix the label");
  await commit.click();
  await p.waitForTimeout(700);
  check(
    "once the index is empty and a commit is waiting, the button becomes Push",
    (await push.count()) === 1 && (await commit.count()) === 0,
  );
  check(
    "the commit box says what a push sends",
    (await p.getByText("1 commit to push to origin").count()) === 1,
  );

  await p.getByRole("button", { name: "Generate message with AI" }).click();
  await p.waitForTimeout(200);
  check(
    "with nothing staged, Generate still answers the click and says what to do",
    (await p
      .getByText(
        "Stage the files you want to commit, then generate the message.",
      )
      .count()) === 1,
  );

  await p.getByRole("button", { name: "Commits", exact: true }).click();
  await p.waitForTimeout(300);
  const history = p.getByRole("list", { name: "Commits on this branch" });
  check(
    "the Commits view lists the commits made, newest first, with the unpushed one marked",
    (
      (await history.getByRole("listitem").first().textContent()) ?? ""
    ).includes("Fix the label") &&
      (
        (await history.getByRole("listitem").first().textContent()) ?? ""
      ).includes("Not pushed") &&
      (await history.getByRole("listitem").count()) === 4,
  );
  const summaryBox = await p
    .getByRole("textbox", { name: "Commit summary" })
    .boundingBox();
  const listBox = await history.boundingBox();
  check(
    "the commit box stays above the list, which sits below it",
    !!summaryBox && !!listBox && summaryBox.y < listBox.y,
    JSON.stringify({ summaryBox, listBox }),
  );
  await history.getByRole("button", { name: /Fix the label/ }).click();
  await p.waitForTimeout(300);
  const commitFiles = p.getByRole("list", { name: /^Files in commit/ });
  check(
    "a commit expands into its files, each with its kind and lines",
    (await commitFiles.getByRole("listitem").count()) === 2 &&
      (
        (await commitFiles.getByRole("listitem").first().textContent()) ?? ""
      ).includes("+3"),
  );
  await commitFiles.getByRole("button", { name: /App\.tsx/ }).click();
  await p.locator(".cm-editor").waitFor({ timeout: 5000 });
  const commitToolbar = p.getByRole("toolbar", { name: "File view" });
  check(
    "a commit file opens as a diff tab naming the commit, with Edit off and no compare switch",
    ((await commitToolbar.textContent()) ?? "").includes("Commit 1004") &&
      (await commitToolbar
        .getByRole("button", { name: "Edit" })
        .isDisabled()) === true &&
      (await commitToolbar
        .getByRole("group", { name: "Compare with" })
        .count()) === 0,
  );

  await push.click();
  await p.waitForTimeout(700);
  check(
    "after the push the button is Commit again and the push is reported",
    (await commit.count()) === 1 &&
      (await push.count()) === 0 &&
      (await p.getByText("Pushed to origin.").count()) === 1 &&
      (await history.getByText("Not pushed").count()) === 0,
  );
});

// A branch with no pull request: the form sits in the tab, like the commit box, and creates it.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^refactor-db/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(700);

  const title = p.getByRole("textbox", { name: "Pull request title" });
  const body = p.getByRole("textbox", { name: "Pull request description" });
  check(
    "a branch with no pull request shows the form in the tab, not a dialog",
    (await p.getByRole("heading", { name: "Open a pull request" }).count()) ===
      1 &&
      (await p.getByRole("dialog").count()) === 0 &&
      (await title.inputValue()) === "Refactor db pool" &&
      (
        (await p.getByRole("combobox", { name: "Base branch" }).innerText()) ??
        ""
      ).includes("main"),
  );
  check(
    "a branch the remote lacks says it is pushed first, and the button says so too",
    (await p
      .getByText("refactor/db-pool is pushed to origin first.")
      .count()) === 1 &&
      (await p
        .getByRole("button", { name: "Push and create", exact: true })
        .count()) === 1,
  );

  await p
    .getByRole("button", { name: "Generate title and description with AI" })
    .click();
  await p.waitForTimeout(500);
  check(
    "the sparkle button fills the title and description and names the agent",
    (await title.inputValue()) ===
      "refactor(db): size the pool from the config" &&
      ((await body.inputValue()) ?? "").includes("pool size") &&
      (await p
        .getByText("Drafted with Claude Code. Review it before creating.")
        .count()) === 1,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the create pull request form",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByRole("switch", { name: "Draft" }).click();
  await p.getByRole("button", { name: "Push and create", exact: true }).click();
  await p.waitForTimeout(800);
  check(
    "creating replaces the form with the new pull request, marked as a draft",
    (await p.getByRole("heading", { name: /^#77 refactor\(db\)/ }).count()) ===
      1 &&
      (await p.getByText("Draft", { exact: true }).count()) >= 1 &&
      (await p
        .getByRole("heading", { name: "Open a pull request" })
        .count()) === 0,
  );
});

// Discarding asks first, cancelling changes nothing, and confirming removes the file from the list.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(600);
  const list = p.getByRole("group", {
    name: "Changed files in the selected worktree",
  });
  const app = list.getByRole("treeitem", { name: /^App\.tsx, modified$/ });
  const appText = (await app.textContent()) ?? "";
  check(
    "a changed row shows the lines added and removed",
    appText.includes("+3") && appText.includes("−1"),
    appText,
  );
  await app.click();
  await p.locator(".cm-editor").waitFor({ timeout: 5000 });
  const compare = p
    .getByRole("toolbar", { name: "File view" })
    .getByRole("group", { name: "Compare with" });
  check(
    "a changed file offers HEAD, Staged, Unstaged and Base as what to compare with",
    (await compare.getByRole("button").allTextContents()).join("|") ===
      "HEAD|Staged|Unstaged|Base",
  );
  await compare.getByRole("button", { name: "Staged", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "picking Staged shows the index side of the diff",
    ((await p.locator(".cm-content").first().innerText()) ?? "").includes(
      "staged line",
    ),
  );
  await compare.getByRole("button", { name: "HEAD", exact: true }).click();
  await p.waitForTimeout(300);

  await list
    .getByRole("button", { name: "Discard App.tsx", exact: true })
    .click();
  await p.waitForTimeout(300);
  const dialog = p.getByRole("alertdialog");
  check(
    "discarding asks first and says it cannot be undone",
    (await dialog.getByText("Discard this change?").count()) === 1 &&
      ((await dialog.textContent()) ?? "").includes("cannot be undone"),
  );
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await p.waitForTimeout(300);
  check("cancelling keeps the file in the list", (await app.count()) === 1);

  await app.focus();
  await p.keyboard.press("Delete");
  await p.waitForTimeout(300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Discard", exact: true })
    .click();
  await p.waitForTimeout(500);
  check(
    "confirming removes the file from the Changes section",
    (await app.count()) === 0,
  );

  await list
    .getByRole("button", { name: "Discard all untracked files", exact: true })
    .click();
  await p.waitForTimeout(300);
  check(
    "a section's discard button names how many files it covers",
    (await p
      .getByRole("alertdialog")
      .getByText("Discard this change?")
      .count()) === 1,
  );
});

// Amend: the last commit's message comes back, and a commit the remote has needs a second click.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(600);

  const summary = p.getByRole("textbox", { name: "Commit summary" });
  await p.getByRole("switch", { name: "Amend last commit" }).click();
  await p.waitForTimeout(300);
  check(
    "turning amend on brings back the last commit's message",
    (await summary.inputValue()) === "feat(auth): add the login form",
  );

  await summary.fill("feat(auth): add the sign-in form");
  await p.getByRole("button", { name: "Amend", exact: true }).click();
  await p.waitForTimeout(300);
  check(
    "amending a commit the remote has asks for a second, deliberate click",
    (await p.getByText(/already on origin\. Amending rewrites it/).count()) ===
      1 &&
      (await p
        .getByRole("button", { name: "Amend anyway", exact: true })
        .count()) === 1,
  );

  await p.getByRole("button", { name: "Amend anyway", exact: true }).click();
  await p.waitForTimeout(600);
  await p.getByRole("button", { name: "Commits", exact: true }).click();
  await p.waitForTimeout(300);
  const history = p.getByRole("list", { name: "Commits on this branch" });
  const first =
    (await history.getByRole("listitem").first().textContent()) ?? "";
  check(
    "the amended commit keeps its place, with the new subject, and is no longer on the remote",
    first.includes("add the sign-in form") &&
      first.includes("Not pushed") &&
      (await history.getByRole("listitem").count()) === 3,
    first,
  );
});

// Keeping the branch and its remote together: Pull when behind, Sync when both moved.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(600);

  await p
    .getByRole("textbox", { name: "Commit summary" })
    .fill("Fix the label");
  await p.getByRole("button", { name: "Commit", exact: true }).click();
  await p.waitForTimeout(700);
  await p.getByRole("button", { name: "Push", exact: true }).click();
  await p.waitForTimeout(700);

  await p.evaluate(() => window.__lynxMock.setBehind(2));
  await p.getByRole("button", { name: "Refresh", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "when the remote has commits and the branch has none of its own, the button is Pull",
    (await p.getByRole("button", { name: "Pull", exact: true }).count()) === 1,
  );
  await p.getByRole("button", { name: "Pull", exact: true }).click();
  await p.waitForTimeout(600);
  check(
    "pulling brings the branch level and says so",
    (await p.getByText("Pulled from origin.").count()) === 1 &&
      (await p.getByRole("button", { name: "Commit", exact: true }).count()) ===
        1,
  );

  // Something of its own to push, and the remote moved too: Sync.
  await p.getByRole("button", { name: "Stage README.md", exact: true }).click();
  await p.waitForTimeout(400);
  await p.getByRole("textbox", { name: "Commit summary" }).fill("Second fix");
  await p.getByRole("button", { name: "Commit", exact: true }).click();
  await p.waitForTimeout(700);
  await p.evaluate(() => window.__lynxMock.setBehind(1));
  await p.getByRole("button", { name: "Refresh", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "with commits on both sides the button is Sync, and says what each side has",
    (await p.getByRole("button", { name: "Sync", exact: true }).count()) ===
      1 && (await p.getByText(/1 to push, 1 to pull/).count()) === 1,
  );
  await p.getByRole("button", { name: "Sync", exact: true }).click();
  await p.waitForTimeout(800);
  check(
    "syncing joins the two and ends level",
    (await p.getByText("Synced with origin.").count()) === 1 &&
      (await p.getByRole("button", { name: "Sync", exact: true }).count()) ===
        0,
  );
});

// An amended commit the remote has: a plain push would be refused, so the button is a Force push with lease.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(600);

  await p.getByRole("switch", { name: "Amend last commit" }).click();
  await p.waitForTimeout(300);
  await p.getByRole("button", { name: "Amend", exact: true }).click();
  await p.waitForTimeout(200);
  await p.getByRole("button", { name: "Amend anyway", exact: true }).click();
  await p.waitForTimeout(700);

  const force = p.getByRole("button", { name: "Force push", exact: true });
  check(
    "after rewriting a pushed commit the button is Force push, and nothing else offers to sync",
    (await force.count()) === 1 &&
      (await p.getByRole("button", { name: "Sync", exact: true }).count()) ===
        0 &&
      (await p.getByText(/old copies of commits you rewrote/).count()) === 1,
  );
  await force.click();
  await p.waitForTimeout(300);
  check(
    "a force push asks for a second click and says what it replaces",
    (await p
      .getByRole("button", { name: "Force push anyway", exact: true })
      .count()) === 1 &&
      (await p.getByText(/replaces 1 commit on origin/).count()) === 1,
  );
  await p
    .getByRole("button", { name: "Force push anyway", exact: true })
    .click();
  await p.waitForTimeout(700);
  check(
    "after the force push the branch is level again",
    (await p.getByText("Replaced the branch on origin.").count()) === 1 &&
      (await p
        .getByRole("button", { name: "Force push", exact: true })
        .count()) === 0,
  );
});

// Actions on the pull request: auto-merge, draft state, close and reopen, and a merge that asks twice.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(700);

  check(
    "an open pull request says where it merges and what stands in the way",
    (await p
      .getByText(/Merging into main: checks are failing or still running/)
      .count()) === 1 &&
      (
        (await p.getByRole("combobox", { name: "Merge method" }).innerText()) ??
        ""
      ).includes("Squash and merge"),
  );

  await p.getByRole("button", { name: "Enable auto-merge" }).click();
  await p.waitForTimeout(500);
  check(
    "enabling auto-merge says GitHub will merge it once the requirements pass",
    (await p.getByText(/Auto-merge is on/).count()) === 1,
  );

  await p.getByRole("button", { name: "Convert to draft" }).click();
  await p.waitForTimeout(500);
  check(
    "converting to draft shows the Draft badge and offers Ready for review",
    (await p.getByText("Draft", { exact: true }).count()) >= 1 &&
      (await p.getByRole("button", { name: "Ready for review" }).count()) === 1,
  );
  await p.getByRole("button", { name: "Ready for review" }).click();
  await p.waitForTimeout(500);

  await p.getByRole("button", { name: "Close pull request" }).click();
  await p.waitForTimeout(500);
  check(
    "closing shows the Closed badge and offers to reopen",
    (await p.getByText("Closed", { exact: true }).count()) >= 1 &&
      (await p.getByRole("button", { name: "Reopen pull request" }).count()) ===
        1,
  );
  await p.getByRole("button", { name: "Reopen pull request" }).click();
  await p.waitForTimeout(500);

  await p.getByRole("button", { name: "Merge", exact: true }).click();
  await p.waitForTimeout(200);
  check(
    "a merge asks for a second click and says what it merges where",
    (await p.getByRole("button", { name: "Confirm merge" }).count()) === 1 &&
      (await p.getByText(/Merges #42 into main/).count()) === 1,
  );
  await p.getByRole("button", { name: "Confirm merge" }).click();
  await p.waitForTimeout(600);
  check(
    "after the merge the badge reads Merged and the actions are gone",
    (await p.getByText("Merged", { exact: true }).count()) >= 1 &&
      (await p.getByRole("button", { name: "Close pull request" }).count()) ===
        0,
  );

  // A merged pull request offers to remove its worktree and branch.
  check(
    "a merged pull request offers to clean up its worktree",
    (await p.getByText("This pull request is merged.").count()) === 1 &&
      (await p.getByRole("button", { name: "Clean up worktree" }).count()) ===
        1,
  );
  await p.getByRole("button", { name: "Clean up worktree" }).click();
  await p.waitForTimeout(300);
  const cleanUp = p.getByRole("alertdialog");
  check(
    "the clean-up dialog says what goes: the folder, the branch and the terminals",
    ((await cleanUp.textContent()) ?? "").includes("Clean up feat/auth?") &&
      ((await cleanUp.textContent()) ?? "").includes("branch feat/auth") &&
      ((await cleanUp.textContent()) ?? "").includes("closes its terminals"),
  );
  await p.evaluate(axeSource);
  const cleanUpAxe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations in the clean-up dialog",
    cleanUpAxe.length === 0,
    cleanUpAxe.join(" || "),
  );
  await cleanUp.getByRole("button", { name: "Cancel" }).click();
  await p.waitForTimeout(300);
  check(
    "cancelling keeps the worktree",
    (await p.getByRole("button", { name: /^feat\/auth/ }).count()) >= 1,
  );

  await p.getByRole("button", { name: "Clean up worktree" }).click();
  await p.waitForTimeout(300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Remove worktree and branch" })
    .click();
  await p.waitForTimeout(800);
  check(
    "confirming removes the worktree from the sidebar and says so",
    (await p.getByRole("button", { name: /^feat\/auth/ }).count()) === 0 &&
      (await p.getByText("feat/auth was cleaned up.").count()) === 1,
  );
});

// The Discussion view: the conversation, an inline thread to reply to and resolve, and a new comment.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(700);

  check(
    "the pull request offers Checks and Discussion, with counts",
    (
      await p.getByRole("button", { name: /^Discussion/ }).textContent()
    )?.includes("3") === true,
  );
  await p.getByRole("button", { name: /^Discussion/ }).click();
  await p.waitForTimeout(400);
  check(
    "the discussion shows the inline thread with its place, and the conversation in order",
    (await p.getByText("src/App.tsx:12").count()) === 1 &&
      (await p.getByText("Why is this call made twice?").count()) === 1 &&
      (await p.getByText(/requested changes/).count()) === 1 &&
      (await p
        .getByText("Could we also cover the refresh path in a test?")
        .count()) === 1,
  );

  await p.getByRole("button", { name: "Reply", exact: true }).click();
  await p
    .getByRole("textbox", { name: "Reply in src/App.tsx" })
    .fill("It was a leftover, fixed.");
  await p.getByRole("button", { name: "Reply", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "replying adds the answer to the thread",
    (await p.getByText("It was a leftover, fixed.").count()) === 1,
  );

  await p.getByRole("button", { name: "Resolve thread" }).click();
  await p.waitForTimeout(400);
  check(
    "resolving marks the thread resolved and offers to reopen it",
    (await p.getByText("Resolved", { exact: true }).count()) === 1 &&
      (await p.getByRole("button", { name: "Reopen thread" }).count()) === 1,
  );

  await p
    .getByRole("textbox", { name: "Add a comment" })
    .fill("Thanks, looks good now.");
  await p.getByRole("button", { name: "Comment", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "a new comment joins the conversation and clears the box",
    (await p.getByText("Thanks, looks good now.").count()) === 1 &&
      (await p.getByRole("textbox", { name: "Add a comment" }).inputValue()) ===
        "",
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations in the discussion",
    axe.length === 0,
    axe.join(" || "),
  );
});

// A failed check has a log to read in place.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(700);

  check(
    "only a GitHub Actions job offers a log, and every check with a page offers Open",
    (await p.getByRole("button", { name: "Log of CI / test" }).count()) === 1 &&
      (await p.getByRole("button", { name: /^Log of / }).count()) === 1 &&
      (await p
        .getByRole("button", { name: "Open CI / test on GitHub" })
        .count()) === 1,
  );
  await p.getByRole("button", { name: "Log of CI / test" }).click();
  await p.waitForTimeout(400);
  const log = p.getByLabel("Log text of CI / test");
  check(
    "opening the log shows its end, says it was cut, and can be closed",
    ((await log.textContent()) ?? "").includes("test failed") &&
      (await p.getByText("Only the end of the log is shown.").count()) === 1,
  );
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with a log open",
    axe.length === 0,
    axe.join(" || "),
  );
  await p.getByRole("button", { name: "Log of CI / test" }).click();
  check("hiding the log removes it", (await log.count()) === 0);
});

// Fix with AI: a rejected commit and broken checks are handed to an agent that is on.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(500);
  const summary = p.getByRole("textbox", { name: "Commit summary" });
  const fix = p.getByRole("button", { name: "Fix with AI" });
  const failCommit = async () => {
    await p
      .getByRole("group", { name: "Changed files in the selected worktree" })
      .getByRole("button", { name: "Stage App.tsx" })
      .click();
    await p.waitForTimeout(300);
    await summary.fill("Ship it, hook fails");
    await p.getByRole("button", { name: "Commit", exact: true }).click();
    await p.waitForTimeout(500);
  };

  await failCommit();
  check(
    "a rejected commit shows git's words and no fix button while no agent is on",
    (await p.getByText("pre-commit hook failed").count()) === 1 &&
      (await fix.count()) === 0,
  );

  // Turn Claude Code on in Settings, then try again.
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await p.waitForTimeout(300);
  await p.getByRole("switch", { name: "Use Claude Code in Lynx" }).click();
  await p.waitForTimeout(300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Turn on", exact: true })
    .click();
  await p.waitForTimeout(600);
  await p.keyboard.press("Escape");
  await p.waitForTimeout(300);

  await p.getByRole("button", { name: "Commit", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "with an agent on, a rejected commit offers Fix with AI",
    (await fix.count()) === 1,
  );

  await fix.click();
  await p.waitForTimeout(700);
  const run = await p.evaluate(() => window.__lynxMock.lastAgentRun());
  check(
    "Fix with AI writes the error, the message and the staged files to a prompt",
    run.prompt.includes("pre-commit hook failed: eslint found 2 errors") &&
      run.prompt.includes("Ship it, hook fails") &&
      run.prompt.includes("- src/App.tsx") &&
      run.prompt.includes("Do not bypass hooks"),
    run.prompt,
  );
  check(
    "and starts the agent with only a short line that points at the file",
    run.command ===
      "claude 'Read /tmp/lynx-prompts/prompt-mock.md and follow the instructions in it.'",
    String(run.command),
  );
});

await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await p.waitForTimeout(700);
  check(
    "Fix broken checks is not offered while no agent is on",
    (await p.getByRole("button", { name: "Fix broken checks" }).count()) === 0,
  );

  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await p.waitForTimeout(300);
  await p.getByRole("switch", { name: "Use Claude Code in Lynx" }).click();
  await p.waitForTimeout(300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Turn on", exact: true })
    .click();
  await p.waitForTimeout(600);
  await p.keyboard.press("Escape");
  await p.waitForTimeout(300);

  await p.getByRole("button", { name: "Fix broken checks" }).click();
  await p.waitForTimeout(700);
  const run = await p.evaluate(() => window.__lynxMock.lastAgentRun());
  check(
    "Fix broken checks hands the failing checks, with their links, to the agent",
    run.prompt.includes("- CI / test: https://github.com/") &&
      run.prompt.includes("gh run view --job") &&
      run.command?.startsWith("claude 'Read "),
    run.prompt,
  );
});

// System notifications: only for done and needs-you, only while the window is not in front.
await session("?mock=full", 1400, 800, async (p) => {
  const sent = () => p.evaluate(() => window.__lynxMock.notifications());
  const report = (status) =>
    p.evaluate((next) => window.__lynxMock.setAgentStatus(next), status);
  const inFront = (front) =>
    p.evaluate((value) => {
      document.hasFocus = () => value;
    }, front);

  await inFront(true);
  await report("done");
  await p.waitForTimeout(300);
  check(
    "no notification while the window is in front",
    (await sent()).length === 0,
  );

  await inFront(false);
  await report("working");
  await p.waitForTimeout(200);
  check(
    "a working agent is not worth a notification",
    (await sent()).length === 0,
  );

  await report("done");
  await p.waitForTimeout(400);
  const finished = await sent();
  check(
    "an agent that finishes while the window is away is announced, with its place",
    finished.length > 0 &&
      finished.every(
        (item) => item.title.endsWith("finished") && item.body.includes("·"),
      ),
    JSON.stringify(finished),
  );

  await report("done");
  await p.waitForTimeout(200);
  check(
    "the same status twice does not notify twice",
    (await sent()).length === finished.length,
  );

  await report("needsYou");
  await p.waitForTimeout(300);
  const all = await sent();
  check(
    "an agent that needs the user is announced too",
    all.length > finished.length &&
      all
        .slice(finished.length)
        .every((item) => item.title.endsWith("needs you")),
    JSON.stringify(all),
  );

  // Turn "Agent finished" off in Settings: only the needs-you kind stays.
  await inFront(true);
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Notifications", exact: true }).click();
  await p.waitForTimeout(300);
  const finishedSwitch = p.getByRole("switch", { name: "Agent finished" });
  check(
    "the Notifications page has both kinds, on by default",
    (await finishedSwitch.getAttribute("aria-checked")) === "true" &&
      (await p
        .getByRole("switch", { name: "Agent needs you" })
        .getAttribute("aria-checked")) === "true",
  );
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Notifications page",
    axe.length === 0,
    axe.join(" || "),
  );
  await finishedSwitch.click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Escape");
  await p.waitForTimeout(300);

  await inFront(false);
  const before = (await sent()).length;
  await report("working");
  await report("done");
  await p.waitForTimeout(300);
  check(
    "with 'Agent finished' off, finishing is silent",
    (await sent()).length === before,
  );
  await report("needsYou");
  await p.waitForTimeout(300);
  check("and 'Agent needs you' still speaks", (await sent()).length > before);
});

// The file watcher: what an agent writes shows up in Files and Changes without leaving the window.
await session("?mock=full", 1400, 800, async (p) => {
  const worktree = "/home/dev/lynx-ide-worktrees/feat-auth";
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.waitForTimeout(400);
  await p.keyboard.press("Control+Shift+E");
  await p.waitForTimeout(600);

  check(
    "the new file is not listed before the agent writes it",
    (await p.getByRole("treeitem", { name: /^agent-notes\.md/ }).count()) === 0,
  );

  await p.evaluate((path) => {
    window.__lynxMock.agentWritesFile(path, "", "agent-notes.md");
  }, worktree);
  await p.waitForTimeout(500);
  check(
    "a file an agent writes appears in the explorer by itself",
    (await p.getByRole("treeitem", { name: /^agent-notes\.md/ }).count()) === 1,
  );

  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await p.waitForTimeout(500);
  check(
    "and in Changes, as an untracked file",
    (await p
      .getByRole("tree", { name: /^Untracked files/ })
      .getByRole("treeitem", { name: /^agent-notes\.md/ })
      .count()) === 1,
  );

  await p.evaluate((path) => {
    window.__lynxMock.agentWritesFile(path, "docs", "plan.md");
  }, worktree);
  await p.waitForTimeout(500);
  check(
    "a second file shows up in Changes too, with no reload",
    (await p
      .getByRole("tree", { name: /^Untracked files/ })
      .getByRole("treeitem", { name: /^plan\.md/ })
      .count()) === 1,
  );
});

console.log(results.join("\n"));
await browser.close();
vite.kill();
process.exit(results.some((line) => line.startsWith("FAIL")) ? 1 : 0);
