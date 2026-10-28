/**
 * Drives the interface in headless Chrome against the in-memory backend
 * (`?mock=`), so no desktop window is involved. Run with `bun run e2e`.
 * Set CHROME_PATH when Chrome is not at a standard location.
 */
import { spawn } from "node:child_process";
import { AsyncLocalStorage } from "node:async_hooks";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { chromium } from "playwright-core";

const PORT = Number(process.env.E2E_PORT ?? 1431);
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
// Sessions run in parallel, so each one collects its own results; they are printed in
// the order the sessions were declared.
const SESSION_CONCURRENCY = Number(process.env.E2E_CONCURRENCY ?? 4);
const resultsOfSession = new AsyncLocalStorage();
const results = [];
const check = (name, ok, detail = "") =>
  (resultsOfSession.getStore() ?? results).push(
    `${ok ? "PASS" : "FAIL"}  ${name}${ok ? "" : "  -> " + detail}`,
  );

// Resolves once the DOM has been quiet for QUIET_MS (the old fixed sleep is the ceiling).
const QUIET_MS = 80;
// Lazy chunks come over the network (and the dev server transforms them first).
const requestsInFlight = new WeakMap();
const watchRequests = (page) => {
  const state = { count: 0, last: 0 };
  requestsInFlight.set(page, state);
  const touch = (delta) => () => {
    state.count += delta;
    state.last = Date.now();
  };
  page.on("request", touch(1));
  page.on("requestfinished", touch(-1));
  page.on("requestfailed", touch(-1));
};
const settle = async (page, ceilingMs) => {
  const started = Date.now();
  const inFlight = requestsInFlight.get(page);
  const networkBusy = () =>
    inFlight && (inFlight.count > 0 || Date.now() - inFlight.last < 100);
  do {
    while (networkBusy() && Date.now() - started < 10000) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    await page.evaluate(
      ([quiet, ceiling]) =>
        new Promise((resolve) => {
          let timer;
          const done = () => {
            resolve();
            observer.disconnect();
            clearTimeout(cap);
            clearTimeout(timer);
          };
          const arm = () => {
            clearTimeout(timer);
            timer = setTimeout(() => {
              // A command in flight, or a transition still running, will change the page.
              const animating = document
                .getAnimations()
                .some(
                  (a) =>
                    a.playState === "running" &&
                    a.effect?.getComputedTiming().iterations !== Infinity,
                );
              if (animating || window.__lynxMock?.pendingCommands?.() > 0)
                arm();
              else done();
            }, quiet);
          };
          const observer = new MutationObserver(arm);
          observer.observe(document.documentElement, {
            subtree: true,
            childList: true,
            attributes: true,
            characterData: true,
          });
          const cap = setTimeout(done, ceiling);
          requestAnimationFrame(() => requestAnimationFrame(arm));
        }),
      [QUIET_MS, ceilingMs],
    );
    // A lazy chunk is requested only after the render that needs it.
    await new Promise((resolve) => setTimeout(resolve, 30));
  } while (networkBusy() && Date.now() - started < 10000);
};

// The DOM renderer keeps terminal text in the page, which many checks read. So sessions
// run without WebGL unless they ask for it; the WebGL session below covers that path.
async function runSession(query, width, height, run, contextOptions = {}) {
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
  watchRequests(page);
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(BASE + query, { waitUntil: "networkidle" });
  await page.waitForSelector("#root > *");
  await settle(page, 800);
  try {
    await run(page);
  } catch (error) {
    check(
      `session ran to the end (${query} @${width})`,
      false,
      String(error).slice(0, 200),
    );
  }
  check(
    `no console errors (${query} @${width})`,
    errors.length === 0,
    errors.join(" | "),
  );
  await context.close();
}
const queuedSessions = [];
const session = (...args) => {
  queuedSessions.push(args);
};
const runQueuedSessions = async () => {
  const outcomes = queuedSessions.map(() => []);
  let next = 0;
  const worker = async () => {
    while (next < queuedSessions.length) {
      const index = next++;
      await resultsOfSession.run(outcomes[index], () =>
        runSession(...queuedSessions[index]),
      );
    }
  };
  await Promise.all(Array.from({ length: SESSION_CONCURRENCY }, worker));
  for (const outcome of outcomes) results.push(...outcome);
};
const tabs = (page) => page.getByRole("tab").count();
const overflow = (page) =>
  page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);

// Under WebGL the terminal's text is drawn on a canvas, so the page cannot read it.
// With copy-on-select on, a drag over the screen copies the buffer text instead.
const setTerminalSwitch = async (p, id, on) => {
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Behavior", exact: true }).click();
  const toggle = p.locator(`#${id}`);
  if ((await toggle.getAttribute("aria-checked")) !== String(on)) {
    await toggle.click();
  }
  await settle(p, 300);
  await p.getByRole("button", { name: "Close settings" }).click();
  await settle(p, 300);
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
  await settle(p, 200);
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
  await settle(p, 300);
  check(
    "Ctrl+Shift+T opens a third tab",
    (await tabs(p)) === 3,
    String(await tabs(p)),
  );

  await p.keyboard.press("Control+Shift+W");
  await settle(p, 300);
  check(
    "Ctrl+Shift+W closes the active tab",
    (await tabs(p)) === 2,
    String(await tabs(p)),
  );

  await p.keyboard.press("Control+Shift+B");
  await settle(p, 200);
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
  await settle(p, 120);
  const onClientX = await p.evaluate(
    () => document.activeElement?.textContent ?? "",
  );
  check(
    "ArrowDown moves from a folder to the folder inside it",
    onClientX.includes("Client X"),
    onClientX,
  );
  await p.keyboard.press("ArrowDown");
  await settle(p, 120);
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
  await settle(p, 250);
  check(
    "dialog closes immediately and a progress row appears",
    (await p.getByText("Creating feature/payments…").count()) === 1,
  );
  // The worktree appears once Git has made it, which takes a moment under load.
  const created = p.getByRole("button", { name: /^feature\/payments/ });
  await created.waitFor({ timeout: 5000 });
  check(
    "created worktree appears and becomes selected",
    (await created.count()) === 1 &&
      (await created.getAttribute("aria-current")) === "true",
  );

  await p.getByRole("button", { name: /^fix\/pty/ }).hover();
  await p.getByRole("button", { name: /^fix\/pty/ }).click({ button: "right" });
  await settle(p, 300);
  check(
    "right-clicking a worktree opens its actions menu",
    (await p.getByRole("menuitem", { name: /Remove worktree/ }).count()) === 1,
  );
  await p.keyboard.press("Escape");
  await settle(p, 200);
  await p.getByRole("button", { name: "Actions for fix/pty" }).click();
  await p.getByRole("menuitem", { name: /Remove worktree/ }).click();
  await p.getByRole("button", { name: "Remove", exact: true }).click();
  await settle(p, 400);
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

  await settle(p, 300);
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
  await settle(p, 800);

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
  await settle(p, 300);
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
  await settle(p, 200);
  check(
    "clicking the active view hides the side panel and keeps the primary bar",
    (await panel.count()) === 0 &&
      (await nav.count()) === 1 &&
      (await projects.getAttribute("aria-pressed")) === "false",
  );

  await projects.click();
  await settle(p, 200);
  check("clicking it again brings the panel back", (await panel.count()) === 1);

  await p.keyboard.press("Control+Shift+B");
  await settle(p, 200);
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
  await settle(p, 400);
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
  await settle(p, 300);
  check(
    "Escape closes settings",
    (await p.getByRole("dialog", { name: "Settings" }).count()) === 0,
  );

  await p.keyboard.press("Control+,");
  await settle(p, 300);
  check(
    "Ctrl+, opens settings",
    (await p.getByRole("dialog", { name: "Settings" }).count()) === 1,
  );
  await p.getByRole("button", { name: "Close settings" }).click();
  await settle(p, 300);
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
  await settle(p, 600);
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await settle(p, 300);

  await p.getByRole("switch", { name: "Use Claude Code in Lynx" }).click();
  await settle(p, 300);
  await settle(p, 600);

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
  await settle(p, 300);
  check(
    "turning the arguments on shows the text box",
    (await argumentsBox.count()) === 1,
  );
  await argumentsBox.fill("--model opus");
  await argumentsBox.press("Enter");
  await settle(p, 400);
  check(
    "the arguments typed in the box are kept",
    (await argumentsBox.inputValue()) === "--model opus",
  );

  await p.getByRole("combobox", { name: "Launch mode" }).click();
  await p.getByRole("option", { name: "YOLO", exact: true }).click();
  await settle(p, 300);
  const confirm = p.getByRole("alertdialog");
  check(
    "YOLO asks before it is turned on",
    (await confirm.count()) === 1 &&
      ((await confirm.textContent()) ?? "").includes("YOLO mode"),
  );
  await confirm.getByRole("button", { name: "Turn on YOLO" }).click();
  await settle(p, 500);
  check(
    "in YOLO mode the agent says which flag it starts with",
    (await p
      .getByText("YOLO mode adds --dangerously-skip-permissions before them.")
      .count()) === 1,
  );

  await p.keyboard.press("Escape");
  await settle(p, 300);
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
  const menuFontSize = await p
    .getByRole("menuitem", { name: /^Shell/ })
    .evaluate((el) => getComputedStyle(el).fontSize);
  check(
    "the new terminal menu uses the app's text size",
    menuFontSize === "13px",
    menuFontSize,
  );
  await p.getByRole("menuitem", { name: "Claude Code" }).click();
  await settle(p, 500);
  check(
    "a Claude Code terminal opens from the menu",
    (await p.getByRole("tab", { name: /claude/i }).count()) >= 1,
  );
});

// The agents found on this computer get a switch, and flipping it takes effect without a confirmation.
await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 600);
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await settle(p, 300);

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

  // Turning an agent on needs no confirmation: the switch itself is the yes.
  await claudeSwitch.click();
  await settle(p, 400);
  check(
    "turning Claude Code on asks nothing and takes effect at once",
    (await p.getByRole("alertdialog").count()) === 0 &&
      (await claudeSwitch.getAttribute("aria-checked")) === "true",
  );
  check(
    "the browser tools went in with the hooks",
    (await p.evaluate(() => window.__lynxMock.browserToolsOn("claude"))) ===
      true,
  );

  await claudeSwitch.click();
  await settle(p, 400);
  check(
    "turning it off asks nothing either",
    (await p.getByRole("alertdialog").count()) === 0 &&
      (await claudeSwitch.getAttribute("aria-checked")) === "false" &&
      (await p.evaluate(() => window.__lynxMock.browserToolsOn("claude"))) ===
        false,
  );

  await claudeSwitch.click();
  await settle(p, 400);

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
  await settle(p, 600);
  await p.keyboard.press("Control+,");
  await settle(p, 400);
  await p.getByRole("tab", { name: "Text", exact: true }).waitFor();

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
  await settle(p, 500);
  check(
    "a step up saves the size and restyles the open terminal",
    (await sizeText()) === "14" && (await termFont(p))?.size === "14px",
    `${await sizeText()} ${JSON.stringify(await termFont(p))}`,
  );
  const reset = p.getByRole("button", { name: "Reset to default" });
  check("a changed setting offers a reset", (await reset.count()) === 1);
  await reset.click();
  await settle(p, 500);
  check(
    "reset returns the size to the default",
    (await sizeText()) === "13" && (await termFont(p))?.size === "13px",
  );

  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 32 }));
  await settle(p, 400);
  check("the larger button stops at the maximum", await increase.isDisabled());
  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 8 }));
  await settle(p, 400);
  check("the smaller button stops at the minimum", await decrease.isDisabled());
  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 20 }));
  await settle(p, 400);
  check(
    "an edit made in the file shows up in the stepper and the terminal",
    (await sizeText()) === "20" && (await termFont(p))?.size === "20px",
    `${await sizeText()} ${JSON.stringify(await termFont(p))}`,
  );

  const family = p.locator("#terminal-font-family");
  await family.click();
  await p.getByRole("option", { name: "Fira Code" }).click();
  await settle(p, 500);
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
  await settle(p, 300);
  check(
    "a font list that tries to inject CSS is refused",
    (await p
      .getByRole("alert")
      .filter({ hasText: "comma-separated" })
      .count()) === 1,
  );
  await typed.fill("Hack, monospace");
  await p.keyboard.press("Enter");
  await settle(p, 500);
  check(
    "a typed font list is saved and goes first",
    (await termFont(p))?.family.includes("Hack") === true &&
      (await p.getByRole("alert").count()) === 0,
  );
  await family.click();
  await p.getByRole("option", { name: /JetBrains Mono/ }).click();
  await settle(p, 400);
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
    await settle(p, 400);
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
  await settle(p, 400);
  check(
    "the chosen shell is shown and saved without an error",
    ((await shell.textContent()) ?? "").includes("/bin/zsh") &&
      (await p.getByRole("alert").count()) === 0,
  );

  await p.getByRole("tab", { name: "Text", exact: true }).click();
  const scrollback = p.locator("#terminal-scrollback");
  await scrollback.click();
  await p.getByRole("option", { name: /10[,.]000/ }).click();
  await settle(p, 400);
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
  await settle(p, 300);
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
  await settle(p, 300);
  check(
    "the bell can be set to nothing",
    /Nothing/.test((await bell.textContent()) ?? ""),
  );

  await p.evaluate(() => window.__lynxMock.editConfigFile({ fontSize: 13 }));
  await p.getByRole("tab", { name: "Worktrees" }).click();
  const directory = p.locator("#worktrees-directory");
  await directory.fill("relative/path");
  await directory.blur();
  await settle(p, 300);
  check(
    "a relative worktrees folder is refused",
    (await p
      .getByRole("alert")
      .filter({ hasText: "absolute path" })
      .count()) === 1,
  );
  await directory.fill("~/trees");
  await directory.blur();
  await settle(p, 300);
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
  await settle(p, 400);
  check(
    "Choose folder saves the folder that was picked",
    (await directory.inputValue()) === "/home/dev/web-app",
  );

  check(
    "the footer shows the config file path",
    (await p.getByText("/home/dev/.lynx/config.json").count()) >= 1,
  );
  await p.getByRole("button", { name: "Open config file" }).click();
  await settle(p, 200);
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
  await settle(p, 500);

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
  await settle(p, 100);
  check(
    "a bell flashes the terminal by default",
    (await p.locator(".terminal-bell").count()) === 1,
  );
  await settle(p, 600);
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Behavior", exact: true }).click();
  await p.locator("#terminal-bell").click();
  await p.getByRole("option", { name: "Nothing" }).click();
  await p.getByRole("button", { name: "Close settings" }).click();
  await settle(p, 300);
  await p.locator(".xterm-helper-textarea:visible").focus();
  await p.keyboard.press("Control+g");
  await settle(p, 100);
  check(
    "with the bell set to nothing, nothing flashes",
    (await p.locator(".terminal-bell").count()) === 0,
  );

  await setTerminalSwitch(p, "terminal-confirm-close", true);
  await p
    .getByRole("tab", { name: /claude/ })
    .locator("[data-close]")
    .click();
  await settle(p, 300);
  check(
    "closing a tab asks first when confirmation is on",
    (await p.getByRole("alertdialog").count()) === 1,
  );
  await p.getByRole("button", { name: "Keep it open" }).click();
  await settle(p, 300);
  check(
    "keeping it open leaves the tab where it was",
    (await p.getByRole("tab", { name: /claude/ }).count()) === 1 &&
      (await p.getByRole("alertdialog").count()) === 0,
  );
  await p
    .getByRole("tab", { name: /claude/ })
    .locator("[data-close]")
    .click();
  await settle(p, 300);
  await p.getByRole("button", { name: "Close terminal" }).click();
  await settle(p, 400);
  check(
    "confirming closes the tab",
    (await p.getByRole("tab", { name: /claude/ }).count()) === 0,
  );

  await setTerminalSwitch(p, "terminal-close-on-exit", true);
  await p.evaluate(() => window.__lynxMock.endAllTerminals());
  await settle(p, 500);
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
  // The settings code loads the first time the dialog opens, which can take longer than a fixed wait.
  await p.getByRole("tab", { name: "Terminal", exact: true }).waitFor();
  await settle(p, 400);
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
    await settle(p, 500);
    const panel = p.getByRole("complementary", { name: "Side panel" });
    const tree = p.getByRole("tree", {
      name: "Files in the selected worktree",
    });
    const item = (name) => p.getByRole("treeitem", { name });

    check("the side panel starts closed", (await panel.count()) === 0);
    await p.keyboard.press("Control+Shift+E");
    await settle(p, 500);
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
    await item(/^README\.md(,|$)/).waitFor();
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
    await settle(p, 400);
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
    await settle(p, 200);
    check(
      "clicking it again closes it",
      (await item(/^App\.tsx(,|$)/).count()) === 0,
    );

    check(
      "ignored files are announced as ignored",
      (await item(/node_modules.*ignored by Git/).count()) === 1,
    );
    await item(/^empty-folder$/).click();
    await settle(p, 400);
    check(
      "an empty folder says so",
      (await p.getByText("Empty", { exact: true }).count()) === 1,
    );
    await item(/^empty-folder$/).click();

    // Keyboard: the tree behaves like a tree.
    await item(/^docs(,|$)/).click();
    await settle(p, 400);
    await p.keyboard.press("ArrowDown");
    await settle(p, 120);
    check(
      "ArrowDown moves into the opened folder",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) === "docs/architecture.md",
    );
    await p.keyboard.press("ArrowLeft");
    await settle(p, 120);
    check(
      "ArrowLeft goes back up to the parent folder",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) === "docs",
    );
    await p.keyboard.press("ArrowLeft");
    await settle(p, 200);
    check(
      "ArrowLeft on an open folder closes it",
      (await item(/^docs(,|$)/).getAttribute("aria-expanded")) === "false",
    );
    await p.keyboard.press("ArrowRight");
    await settle(p, 300);
    check(
      "ArrowRight opens it again",
      (await item(/^docs(,|$)/).getAttribute("aria-expanded")) === "true",
    );
    await p.keyboard.press("End");
    await settle(p, 120);
    check(
      "End goes to the last row",
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-tree-path"),
      )) !== "docs",
    );
    await p.keyboard.press("Home");
    await settle(p, 120);
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
    await settle(p, 400);
    // Every session stays mounted; readTerminal reads the one on screen.
    const typed = await readTerminal(p);
    check(
      "Send path to terminal types the path at the prompt",
      typed.includes("README.md"),
      typed.slice(-120),
    );

    await item(/^a file with spaces\.md$/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Send path to terminal" }).click();
    await settle(p, 400);
    const typedWithSpaces = await readTerminal(p);
    check(
      "a path with spaces is quoted for the shell",
      typedWithSpaces.includes("'a file with spaces.md'"),
      typedWithSpaces.slice(-160),
    );

    await item(/^docs(,|$)/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Copy relative path" }).click();
    await settle(p, 300);
    check(
      "Copy relative path puts it on the clipboard",
      (await p.evaluate(() => navigator.clipboard.readText())) === "docs",
    );
    await item(/^docs(,|$)/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Copy path", exact: true }).click();
    await settle(p, 300);
    check(
      "Copy path gives the absolute path",
      (await p.evaluate(() => navigator.clipboard.readText())).endsWith(
        "/lynx-ide-worktrees/feat-auth/docs",
      ),
      await p.evaluate(() => navigator.clipboard.readText()),
    );
    await item(/^README\.md(,|$)/).click({ button: "right" });
    await p.getByRole("menuitem", { name: "Open containing folder" }).click();
    await settle(p, 200);
    check(
      "Open containing folder asks the backend with the worktree and the item",
      (await p.evaluate(() => window.__lynxMock.probe.revealed))?.endsWith(
        "::README.md",
      ) ?? false,
    );

    // The menu is reachable without a mouse.
    await item(/^README\.md(,|$)/).focus();
    await p.keyboard.press("Shift+F10");
    await settle(p, 300);
    check(
      "Shift+F10 opens the actions menu for the focused row",
      (await p
        .getByRole("menuitem", { name: "Copy path", exact: true })
        .count()) === 1,
    );
    await p.keyboard.press("Escape");
    await settle(p, 400);

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
    await settle(p, 300);
    check("the shortcut closes the panel again", (await panel.count()) === 0);
  },
  { permissions: ["clipboard-read", "clipboard-write"] },
);

await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  const item = (name) => p.getByRole("treeitem", { name });
  const fileTabs = p.getByRole("tablist", { name: "Open files" });

  await item(/^src(,|$)/).click();
  await settle(p, 300);
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
  await settle(p, 300);
  check(
    "Ctrl+S saves it",
    (await fileTabs.getByText("unsaved changes").count()) === 0,
  );

  await p.keyboard.type("!");
  check(
    "typing again marks it unsaved",
    (await fileTabs.getByText("unsaved changes").count()) === 1,
  );
  await p
    .getByText("unsaved changes")
    .waitFor({ state: "detached", timeout: 4000 })
    .catch(() => {});
  check(
    "it saves by itself after a pause in typing",
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
  await settle(p, 300);
  check(
    "a file changed on disk shows the conflict instead of saving",
    (await p
      .getByRole("alert")
      .filter({ hasText: "changed on disk" })
      .count()) === 1,
  );
  await p.getByRole("button", { name: "Reload from disk" }).click();
  await settle(p, 300);
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
  await settle(p, 400);
  check(
    "Enter opens the selected file from the keyboard",
    (await fileTabs.getByRole("tab").count()) === 2,
  );

  await p
    .getByRole("tablist", { name: "Terminals" })
    .getByRole("tab")
    .first()
    .click();
  await settle(p, 300);
  check(
    "choosing a terminal tab brings the terminal back, and the files stay open",
    (await p.locator(".cm-editor").count()) === 0 &&
      (await fileTabs.getByRole("tab").count()) === 2,
  );

  await item(/^logo\.png$/).dblclick();
  await settle(p, 400);
  check(
    "an image opens in the viewer, not the editor",
    (await p.getByRole("img", { name: "Image logo.png" }).count()) === 1 &&
      (await p.locator(".cm-editor").count()) === 0,
  );

  await item(/^bundle\.zip$/).dblclick();
  await settle(p, 400);
  check(
    "a file that is not text says so instead of opening",
    (await p.getByRole("alert").filter({ hasText: "not text" }).count()) === 1,
  );

  await fileTabs.getByRole("tab", { name: /App\.tsx/ }).click();
  // Typing saves by itself, so a change stays unsaved only when the disk copy changed under it.
  await p.evaluate(() => {
    window.__lynxMock.touchFile(
      "/home/dev/lynx-ide-worktrees/feat-auth",
      "src/App.tsx",
      "changed again\n",
    );
  });
  await p.locator(".cm-content").click();
  await p.keyboard.type("y");
  await fileTabs.getByRole("tab", { name: /App\.tsx/ }).press("Delete");
  await settle(p, 300);
  check(
    "closing a file with unsaved changes asks first",
    (await p.getByRole("alertdialog").count()) === 1,
  );
  await p.getByRole("button", { name: "Close without saving" }).click();
  await settle(p, 300);
  check(
    "confirming closes the file",
    (await fileTabs.getByRole("tab", { name: /App\.tsx/ }).count()) === 0,
  );
});

await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 400);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 500);
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
  await settle(p, 300);
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
  await settle(p, 500);
  check(
    "another single click replaces the temporary tab",
    (await fileTabs.getByRole("tab").count()) === 1 &&
      (await fileTabs.getByRole("tab", { name: /Button\.tsx/ }).count()) === 1,
  );

  await row(/^package\.json, modified$/).dblclick();
  await settle(p, 500);
  const pkgTab = fileTabs.getByRole("tab", { name: /package\.json/ });
  check(
    "double click opens it as a permanent tab",
    (await pkgTab.locator("span.italic").count()) === 0,
  );
  await row(/^App\.tsx, modified$/).click();
  await settle(p, 500);
  check(
    "a permanent tab survives the next temporary one",
    (await fileTabs.getByRole("tab").count()) === 2 &&
      (await pkgTab.count()) === 1 &&
      (await fileTabs.getByRole("tab", { name: /Button\.tsx/ }).count()) === 0,
  );

  const toolbar = p.getByRole("toolbar", { name: "File view" });
  await toolbar.getByRole("button", { name: "Split" }).click();
  await settle(p, 400);
  check(
    "Split shows the original and the new text side by side",
    (await p.locator(".cm-merge-a").count()) === 1 &&
      (await p.locator(".cm-merge-b").count()) === 1,
  );
  await toolbar.getByRole("button", { name: "Unified" }).click();
  await toolbar.getByRole("button", { name: "Edit" }).click();
  await settle(p, 400);
  check(
    "Edit switches the same tab to the editor",
    (await p.locator(".cm-content[contenteditable=true]").count()) === 1 &&
      (await p.locator(".cm-mergeView").count()) === 0,
  );
  await p.locator(".cm-content").click();
  await p.keyboard.type("// local");
  await toolbar.getByRole("button", { name: "Diff" }).click();
  await settle(p, 400);
  check(
    "the diff includes text that is not saved yet",
    (await p.locator(".cm-line", { hasText: "// local" }).count()) >= 1,
  );

  await row(/^Legacy\.tsx, deleted$/).click();
  await settle(p, 500);
  check(
    "a deleted file opens as a diff, and Edit is off because there is nothing to edit",
    (await p.locator(".cm-deletedLine, .cm-deletedChunk").count()) >= 0 &&
      (await toolbar.getByRole("button", { name: "Edit" }).isDisabled()),
  );

  await p.getByRole("tab", { name: "Files", exact: true }).click();
  await settle(p, 300);
  await p.getByRole("treeitem", { name: /^README\.md(,|$)/ }).dblclick();
  await settle(p, 400);
  check(
    "a file opened from Files starts in the editor, and Diff is on because it changed",
    (await toolbar
      .getByRole("button", { name: "Edit", pressed: true })
      .count()) === 1 &&
      (await toolbar.getByRole("button", { name: "Diff" }).isEnabled()),
  );
  await p.getByRole("treeitem", { name: /^LICENSE$/ }).dblclick();
  await settle(p, 400);
  check(
    "Diff is off for a file that did not change",
    await toolbar.getByRole("button", { name: "Diff" }).isDisabled(),
  );

  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 300);
  await row(/^App\.tsx, modified$/).click();
  await settle(p, 500);
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
  await settle(p, 500);
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
  await settle(p, 300);
  check(
    "Escape closes the drawer",
    (await p.getByRole("dialog", { name: "Side panel" }).count()) === 0,
  );
});

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await p.getByRole("tab", { name: /claude/ }).click();
  await settle(p, 500);
  // Other sessions stay mounted under the visible one: a click must still reach the visible terminal.
  await p.evaluate(() => document.activeElement?.blur());
  const box = await p.locator(".xterm-screen:visible").first().boundingBox();
  await p.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await settle(p, 200);
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
    await settle(p, 600);
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
  await settle(p, 400);
  await p.getByText("Select a worktree to browse its files.").waitFor();
  check(
    "without a worktree the panel says what to do",
    (await p.getByText("Select a worktree to browse its files.").count()) === 1,
  );
});

await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 500);
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
  await settle(p, 300);
  check(
    "Delete closes the focused terminal tab",
    (await terminalTabs.count()) === 1,
  );

  await terminalTabs.first().hover();
  await terminalTabs.first().locator("[data-close]").click();
  await settle(p, 300);
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
  await settle(p, 400);
  check(
    "selecting a worktree does not hide the other worktrees' agents",
    (await p.locator("[data-agent-group]").count()) === 3,
  );

  await toggleGroup(featPath).click();
  await settle(p, 300);
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
  await settle(p, 400);
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
  await settle(p, 120);
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
  await settle(p, 400);
  check(
    "selecting another worktree keeps the first one's agents in view",
    (await agent(/^claude, idle$/).count()) === 1,
  );
  check(
    "the newly selected worktree's agents stay collapsed until opened",
    (await agent(/^codex, needs you$/).count()) === 0,
  );
  await toggleGroup(fixPath).click();
  await settle(p, 300);
  check(
    "each row shows its own age",
    ((await agent(/^codex, needs you$/).textContent()) ?? "").includes("3h"),
  );

  await toggleGroup(refactorPath).click();
  await settle(p, 300);
  check(
    "a finished agent is shown as exited, with its age",
    (await agent(/^claude, exited with code 0$/).count()) === 1 &&
      ((await agent(/^claude, exited/).textContent()) ?? "").includes("2d"),
  );

  await agent(/^claude, idle$/).hover();
  await groupOf(featPath).getByRole("button", { name: "Close claude" }).click();
  await settle(p, 400);
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
  await settle(p, 300);
  await p
    .locator(
      '[data-agent-group="/home/dev/lynx-ide-worktrees/feat-auth"] > button',
    )
    .click();
  await settle(p, 300);
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
  await settle(p, 300);
  const actionOpacity = () =>
    p.evaluate(
      () =>
        getComputedStyle(
          document.querySelector('[data-folder-id="f-work"] > div > div'),
        ).opacity,
    );
  const hiddenBefore = await actionOpacity();
  await p.locator('[data-folder-id="f-work"] > div > button').first().hover();
  await settle(p, 300);
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
  await settle(p, 200);
  check(
    "collapsing a folder hides everything inside it",
    (await p.locator('[data-project-id="r2"]').count()) === 0,
  );
  await p.getByRole("button", { name: "Expand Work" }).click();
  await settle(p, 200);
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
  await settle(p, 300);
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
  await settle(p, 300);
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
  await settle(p, 200);
  const blocked = async (id) => p.locator(`[data-option="${id}"]`).isDisabled();
  check("Work cannot be moved inside itself", await blocked("f-work"));
  check(
    "Work cannot be moved inside Client X either",
    await blocked("f-client"),
  );
  check("the top level is always a valid place", !(await blocked("top")));
  await p.keyboard.press("Escape");
  await settle(p, 200);

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
  await settle(p, 300);
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
  await settle(p, 200);
  await p.locator('[data-option="top"]').click();
  await p.getByRole("button", { name: "Move here" }).click();
  await settle(p, 300);
  check(
    "moving a repo to the top level takes it out of its folder",
    (await p.locator('[data-folder-id] [data-project-id="r2"]').count()) ===
      0 && (await p.locator('[data-project-id="r2"]').count()) === 1,
  );

  // Renaming keeps the place.
  await openMenu("Work", "Rename…", true);
  await p.locator("#folder-name").fill("Trabalho");
  await p.keyboard.press("Enter");
  await settle(p, 300);
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

// A folder linked to a directory on disk: its subfolders and repos appear, follow later clones, and unlink.
await session("?mock=full", 1300, 760, async (p) => {
  const tree = p.getByRole("complementary", { name: "Projects" });
  const rowOf = (name) =>
    tree
      .getByRole("button", { name: new RegExp(`^(Collapse|Expand) ${name}$`) })
      .first();
  const menuOf = async (name, isFolder) => {
    await rowOf(name).hover();
    await tree
      .getByRole("button", {
        name: isFolder ? `Actions for folder ${name}` : `Actions for ${name}`,
      })
      .first()
      .click();
  };
  const menuItems = async () =>
    p
      .getByRole("menuitem")
      .evaluateAll((items) => items.map((el) => el.textContent?.trim()));

  await tree
    .getByRole("button", { name: "New folder", exact: true })
    .first()
    .click();
  await p.getByRole("button", { name: "Choose folder…" }).click();
  await p.getByText("2 projects in 1 folder").waitFor({ timeout: 5000 });
  check(
    "choosing a directory names the folder after it and previews what it holds",
    (await p.locator("#folder-name").inputValue()) === "web-app",
  );
  await p.locator("#folder-name").fill("Code");
  await p.getByRole("button", { name: "Create and link" }).click();
  await settle(p, 400);

  check(
    "the linked folder shows its repo and the folder the scan made",
    (await p.locator('[data-project-id]:has-text("lynx")').count()) >= 1 &&
      (await p.locator('[data-folder-id]:has-text("clients")').count()) >= 1,
  );
  check(
    "a repo inside a mirrored folder sits in that folder",
    (await p
      .locator(
        '[data-folder-id]:has-text("clients") [data-project-id]:has-text("acme")',
      )
      .count()) >= 1,
  );

  await menuOf("clients", true);
  const mirroredMenu = await menuItems();
  check(
    "a mirrored folder cannot be deleted from its menu",
    !mirroredMenu.includes("Delete folder…") &&
      mirroredMenu.includes("Rename…"),
    JSON.stringify(mirroredMenu),
  );
  await p.keyboard.press("Escape");

  await menuOf("Code", true);
  const rootMenu = await menuItems();
  check(
    "the linked folder offers Rescan and Unlink",
    rootMenu.includes("Rescan folder") &&
      rootMenu.includes("Unlink from disk…"),
    JSON.stringify(rootMenu),
  );
  await p.keyboard.press("Escape");

  await p.evaluate(() => window.__lynxMock.cloneIntoLinkedFolder("fresh"));
  await menuOf("Code", true);
  await p.getByRole("menuitem", { name: "Rescan folder" }).click();
  await p.getByText("Found changes on disk").waitFor({ timeout: 5000 });
  check(
    "a repo cloned into the directory appears after a rescan",
    (await p.locator('[data-project-id]:has-text("fresh")').count()) === 1,
  );

  await p.evaluate(() => window.__lynxMock.setRepoMissing("fresh", true));
  await menuOf("Code", true);
  await p.getByRole("menuitem", { name: "Rescan folder" }).click();
  await settle(p, 400);
  const missingRow = p.locator('[data-project-id]:has-text("fresh")');
  check(
    "a repo gone from disk says so in words, not only by dimming",
    (await missingRow.textContent())?.includes("Not found") === true,
  );

  const axe = await p.evaluate(async (src) => {
    window.eval(src);
    const r = await window.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa"],
    });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 140)}`,
    );
  }, axeSource);
  check(
    "axe: no WCAG A/AA violations with a linked folder",
    axe.length === 0,
    axe.join(" || "),
  );

  await menuOf("Code", true);
  await p.getByRole("menuitem", { name: "Unlink from disk…" }).click();
  await p.getByRole("button", { name: "Unlink", exact: true }).click();
  await settle(p, 400);
  await menuOf("clients", true);
  const afterUnlink = await menuItems();
  check(
    "after unlinking, the folders it made are ordinary and can be deleted",
    afterUnlink.includes("Delete folder…"),
    JSON.stringify(afterUnlink),
  );
  await p.keyboard.press("Escape");
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
  await settle(p, 200);
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
  await settle(p, 200);
  check(
    "expanding the repo removes the summary again",
    (await spokenOf(repoHeader)) === null,
  );

  await p.getByRole("button", { name: "Collapse Work" }).click();
  await settle(p, 200);
  const folderSpoken = await spokenOf(
    p.locator('[data-folder-id="f-work"] > div > button'),
  );
  check(
    "a collapsed folder counts the projects inside it, at any depth",
    folderSpoken === "1 project",
    folderSpoken ?? "null",
  );
  await p.getByRole("button", { name: "Expand Work" }).click();
  await settle(p, 200);
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
  await settle(p, 300);
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
  await settle(p, 200);
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
  await settle(p, 200);
  check(
    "hovering the header is the only thing that changes it",
    (await header.evaluate((el) => getComputedStyle(el).backgroundColor)) !==
      "rgba(0, 0, 0, 0)",
  );
});

await session("?mock=empty", 1000, 700, async (p) => {
  await p.getByRole("button", { name: "Add project" }).last().click();
  await settle(p, 300);
  await p.getByRole("dialog", { name: "Add project" }).waitFor();
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
  await settle(p, 500);
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
  await settle(p, 300);
  const create = p.getByRole("button", { name: "Create project" });
  check(
    "creating needs a name and a location before it is enabled",
    (await create.isDisabled()) &&
      (await p.getByText("No folder chosen").count()) === 1,
  );

  await p.getByRole("button", { name: "Choose folder…" }).click();
  await settle(p, 300);
  await p.getByLabel("Project name").fill("notes");
  check(
    "the form says where the project folder is created",
    (await p.getByText("/home/dev/web-app", { exact: true }).count()) === 1 &&
      (await create.isEnabled()),
  );

  await settle(p, 300);
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
  await settle(p, 300);
  check(
    "a refused name keeps the form open with the reason and the typed name",
    (await p.getByRole("alert").innerText()).includes("invalid name") &&
      (await p.getByRole("dialog", { name: "Create new project" }).count()) ===
        1 &&
      (await p.getByLabel("Project name").inputValue()) === "a/b",
  );

  await p.getByLabel("Project name").fill("notes");
  await create.click();
  await settle(p, 600);
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
  await settle(p, 400);
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
  await p
    .getByRole("heading", { name: "Projects" })
    .first()
    .waitFor({ state: "detached" });
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

  await settle(p, 300);
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

  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 500);
  const list = p.getByRole("group", {
    name: "Changed files in the selected worktree",
  });
  const group = (name) => p.getByRole("tree", { name });
  const summary = p.getByRole("textbox", { name: "Commit summary" });
  const description = p.getByRole("textbox", { name: "Commit description" });
  const commit = p.getByRole("button", { name: "Commit", exact: true });

  await list.getByRole("button", { name: "Stage App.tsx" }).click();
  await settle(p, 400);
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
  await settle(p, 400);
  check(
    "S on a focused row stages it",
    (await group(/^Staged changes/)
      .getByRole("treeitem", { name: /^setup\.md/ })
      .count()) === 1,
  );

  await list.getByRole("button", { name: "Unstage setup.md" }).click();
  await settle(p, 400);
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
  await settle(p, 400);

  await p.getByRole("button", { name: "Generate message with AI" }).click();
  await settle(p, 500);
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

  await settle(p, 300);
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
  await settle(p, 600);
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

// An agent makes a worktree while the screen is open: the sidebar lists it without a reload.
await session("?mock=full", 1200, 760, async (p) => {
  await p.evaluate(() =>
    window.__lynxMock.worktreeMadeByAgent("lynx-ide", "lynx-task-1a2b3c4d"),
  );
  await settle(p, 400);
  check(
    "a worktree an agent made shows in the sidebar without a reload",
    (await p.getByRole("button", { name: /lynx-task-1a2b3c4d/ }).count()) >= 1,
  );
});

// The Orchestration tab: off until the user turns it on; the coordinator, not Settings, picks the agent and worktree.
await session("?mock=full", 1200, 760, async (p) => {
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Orchestration", exact: true }).click();
  await settle(p, 300);

  check(
    "the Orchestration tab starts off, and finished workers close by default",
    (await p
      .getByRole("switch", { name: "Coordinate agents" })
      .getAttribute("aria-checked")) === "false" &&
      (await p
        .getByRole("switch", { name: "Close finished workers" })
        .getAttribute("aria-checked")) === "true",
  );
  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Orchestration tab",
    axe.length === 0,
    axe.join(" || "),
  );
});

// The Git tab: which agent writes commit messages, the style it follows, and the instructions box.
await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+,");
  await settle(p, 400);
  await p.getByRole("tab", { name: "Git", exact: true }).click();
  await settle(p, 300);

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
  await settle(p, 400);
  const instructions = p.locator("#git-commit-instructions");
  check(
    "choosing Custom shows the instructions box",
    (await instructions.count()) === 1,
  );
  await instructions.fill("Write the subject in Portuguese.");
  await instructions.blur();
  await settle(p, 400);
  check(
    "the instructions save when the box loses focus",
    (await instructions.inputValue()) === "Write the subject in Portuguese.",
  );

  await p.locator("#git-commit-agent").click();
  await p.getByRole("option", { name: "Codex", exact: true }).click();
  await settle(p, 400);
  check(
    "the agent choice saves",
    ((await p.locator("#git-commit-agent").textContent()) ?? "").includes(
      "Codex",
    ),
  );

  await settle(p, 300);
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

// The Browser tab: how far agents may go in the page, and the sites they may visit.
await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+,");
  await settle(p, 400);
  await p.getByRole("tab", { name: "Browser", exact: true }).click();
  await settle(p, 300);

  check(
    "agents get full control of the browser by default, on any site",
    ((await p.locator("#browser-agent-access").textContent()) ?? "").includes(
      "Full control",
    ) && (await p.locator("#browser-allowed-sites").inputValue()) === "",
  );

  await p.locator("#browser-agent-access").click();
  await p.getByRole("option", { name: "Read only", exact: true }).click();
  await settle(p, 400);
  check(
    "the access choice saves",
    ((await p.locator("#browser-agent-access").textContent()) ?? "").includes(
      "Read only",
    ),
  );

  const sites = p.locator("#browser-allowed-sites");
  await sites.fill("example.com\n  docs.rs  \n");
  await sites.blur();
  await settle(p, 400);
  check(
    "the sites save one per line, trimmed",
    (await sites.inputValue()) === "example.com\ndocs.rs",
  );

  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Browser settings",
    axe.length === 0,
    axe.join(" || "),
  );
});

// An agent works the browser in the background; whoever opens it sees who is in control.
// Wide enough for the side panel to dock: as a drawer it is modal and hides the page from getByRole.
await session("?mock=full", 1400, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  check(
    "the browser is not on screen before an agent uses it",
    (await p.getByRole("toolbar", { name: "Browser" }).count()) === 0,
  );

  await p.evaluate(() =>
    window.__lynxMock.agentUsesBrowser("Opening example.com"),
  );
  await settle(p, 500);
  check(
    "an agent using the browser does not take over the screen",
    (await p.getByRole("toolbar", { name: "Browser" }).count()) === 0,
  );
  // The browser opens from one of its pages in the Browser tab of the side panel.
  await p.keyboard.press("Control+Shift+E");
  await p.getByRole("tab", { name: "Browser", exact: true }).click();
  await p.getByRole("button", { name: /^Dashboard · Vite/ }).click();
  await settle(p, 500);
  check(
    "and the toolbar says what the agent is doing",
    (await p
      .getByRole("toolbar", { name: "Browser" })
      .getByRole("status")
      .textContent()) === "Claude Code is in control · Opening example.com",
  );

  await p.waitForTimeout(6400);
  check(
    "the mark goes away a few seconds after the agent stops",
    (await p
      .getByRole("toolbar", { name: "Browser" })
      .getByRole("status")
      .count()) === 0,
  );
});

// A tab of the person can be the one agents use, and a tab in its own window is not drawn in the main view.
await session("?mock=full", 1400, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  // The engine runs, as it does once an agent has used the browser.
  await p.evaluate(() =>
    window.__lynxMock.agentUsesBrowser("Reading the page"),
  );
  await p.keyboard.press("Control+Shift+E");
  await p.getByRole("tab", { name: "Browser", exact: true }).click();
  await p.getByRole("button", { name: /^Dashboard · Vite/ }).click();
  const toolbar = p.getByRole("toolbar", { name: "Browser" });
  const assign = toolbar.getByRole("button", {
    name: "Let the agent use this tab",
  });
  await assign.waitFor();
  check(
    "a tab that is not the agent's offers to become it, to the right of the address",
    (await assign.getAttribute("aria-pressed")) === "false" &&
      (await p.getByText("Agent's tab").count()) === 1 &&
      (await assign.evaluate(
        (button) =>
          button.getBoundingClientRect().left >=
          document
            .querySelector('input[aria-label="Web address"]')
            .getBoundingClientRect().right,
      )),
  );

  await assign.click();
  await toolbar
    .getByRole("button", { name: "The agent uses this tab" })
    .waitFor();
  const dashboard = p.getByRole("button", { name: /^Dashboard · Vite/ });
  check(
    "making it the agent's moves the mark off the old agent tab",
    (await toolbar
      .getByRole("button", { name: "The agent uses this tab" })
      .getAttribute("aria-pressed")) === "true" &&
      (await p.getByText("Agent's tab").count()) === 1 &&
      (await dashboard.getByText("Agent's tab").count()) === 1,
  );

  await toolbar
    .getByRole("button", { name: "Open in a separate window" })
    .click();
  await p
    .getByRole("heading", { name: "This tab is in its own window" })
    .waitFor();
  check(
    "a tab in its own window shows a notice here, and the toolbar leaves it alone",
    (await p.getByRole("textbox", { name: "Web address" }).isDisabled()) &&
      (await toolbar
        .getByRole("button", { name: "Open in a separate window" })
        .isDisabled()),
  );

  await p.getByRole("button", { name: /^Pull request #4/ }).click();
  await settle(p, 300);
  check(
    "another tab takes the notice away",
    (await p.getByText("This tab is in its own window").count()) === 0,
  );
  await dashboard.click();
  await p
    .getByRole("heading", { name: "This tab is in its own window" })
    .waitFor();
  check(
    "choosing the tab in its window brings the notice back",
    (await p
      .getByText("Close that window and the tab comes back here.")
      .count()) === 1,
  );
});

// Without its engine the browser offers to download it, shows how it goes, and then opens.
await session("?mock=full&engine=missing", 1400, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await p.getByRole("tab", { name: "Browser", exact: true }).click();
  await p.getByRole("button", { name: /^Dashboard · Vite/ }).click();
  // Nothing has started the engine yet: the page offers to try again, and that starts it.
  await p.getByRole("button", { name: "Try again", exact: true }).click();
  await settle(p, 500);

  check(
    "a browser with no engine says so and offers the download",
    (await p.getByText("The browser needs its engine").count()) === 1 &&
      (await p.getByRole("button", { name: "Download the engine" }).count()) ===
        1,
  );
  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the engine notice",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByRole("button", { name: "Download the engine" }).click();
  await settle(p, 150);
  const bar = p.getByRole("progressbar");
  check(
    "the download shows a progress bar with the size",
    (await bar.count()) === 1 &&
      (await p.getByText(/Downloading \d+ MB of 139 MB/).count()) === 1,
  );

  await p.waitForTimeout(2200);
  check(
    "when the engine is in, the browser the person asked for opens",
    (await p.getByRole("toolbar", { name: "Browser" }).count()) === 1 &&
      (await p.getByText("The browser needs its engine").count()) === 0,
  );
});

// Settings shows the engine, and removing it makes the browser ask for it again.
await session("?mock=full", 1200, 760, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+,");
  await settle(p, 400);
  await p.getByRole("tab", { name: "Browser", exact: true }).click();
  await settle(p, 400);

  const group = p.getByRole("group", { name: "Browser engine" });
  check(
    "the settings say which engine is installed",
    ((await group.textContent()) ?? "").includes("Installed (154.0.34-p2)"),
  );
  await group.getByRole("button", { name: "Remove" }).click();
  await settle(p, 400);
  check(
    "removing it says it is not installed and offers the download",
    ((await group.textContent()) ?? "").includes("Not installed") &&
      (await group
        .getByRole("button", { name: "Download", exact: true })
        .count()) === 1,
  );
});

// The section buttons stage every file of their section at once.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 500);
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
  await settle(p, 400);
  check(
    "stage all untracked files moves every untracked file into the staged section",
    (await tree(/^Staged changes/)
      .getByRole("treeitem", { name: /^setup\.md/ })
      .count()) === 1 && (await tree(/^Untracked files/).count()) === 0,
  );

  await list
    .getByRole("button", { name: "Stage all changes", exact: true })
    .click();
  await settle(p, 400);
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

  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 600);

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
  await settle(p, 300);
  check(
    "Open on GitHub asks the backend for the pull request link",
    (await p.getByRole("alert").count()) === 0,
  );

  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 600);

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
  await settle(p, 500);
  await p.getByRole("dialog").waitFor();
  const dialog = p.getByRole("dialog");
  // The account and the suggested name load from GitHub after the dialog opens.
  await dialog
    .getByRole("combobox", { name: "Account" })
    .filter({ hasText: "zdearo" })
    .waitFor({ timeout: 5000 });
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
  await settle(p, 400);
  await settle(p, 300);
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
  await settle(p, 700);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 600);

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
  await settle(p, 700);
  check(
    "once the index is empty and a commit is waiting, the button becomes Push",
    (await push.count()) === 1 && (await commit.count()) === 0,
  );
  check(
    "the commit box says what a push sends",
    (await p.getByText("1 commit to push to origin").count()) === 1,
  );

  await p.getByRole("button", { name: "Generate message with AI" }).click();
  await settle(p, 200);
  check(
    "with nothing staged, Generate still answers the click and says what to do",
    (await p
      .getByText(
        "Stage the files you want to commit, then generate the message.",
      )
      .count()) === 1,
  );

  await p.getByRole("button", { name: "Commits", exact: true }).click();
  await settle(p, 300);
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
  await settle(p, 300);
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
  await settle(p, 700);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 700);

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
  await settle(p, 500);
  check(
    "the sparkle button fills the title and description and names the agent",
    (await title.inputValue()) ===
      "refactor(db): size the pool from the config" &&
      ((await body.inputValue()) ?? "").includes("pool size") &&
      (await p
        .getByText("Drafted with Claude Code. Review it before creating.")
        .count()) === 1,
  );

  await settle(p, 300);
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
  await settle(p, 800);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 600);
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
  await settle(p, 500);
  check(
    "picking Staged shows the index side of the diff",
    ((await p.locator(".cm-content").first().innerText()) ?? "").includes(
      "staged line",
    ),
  );
  await compare.getByRole("button", { name: "HEAD", exact: true }).click();
  await settle(p, 300);

  await list
    .getByRole("button", { name: "Discard App.tsx", exact: true })
    .click();
  await settle(p, 300);
  await p.getByRole("alertdialog").waitFor();
  const dialog = p.getByRole("alertdialog");
  check(
    "discarding asks first and says it cannot be undone",
    (await dialog.getByText("Discard this change?").count()) === 1 &&
      ((await dialog.textContent()) ?? "").includes("cannot be undone"),
  );
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await settle(p, 300);
  check("cancelling keeps the file in the list", (await app.count()) === 1);

  await app.focus();
  await p.keyboard.press("Delete");
  await settle(p, 300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Discard", exact: true })
    .click();
  await settle(p, 500);
  check(
    "confirming removes the file from the Changes section",
    (await app.count()) === 0,
  );

  await list
    .getByRole("button", { name: "Discard all untracked files", exact: true })
    .click();
  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 600);

  const summary = p.getByRole("textbox", { name: "Commit summary" });
  await p.getByRole("switch", { name: "Amend last commit" }).click();
  await settle(p, 300);
  check(
    "turning amend on brings back the last commit's message",
    (await summary.inputValue()) === "feat(auth): add the login form",
  );

  await summary.fill("feat(auth): add the sign-in form");
  await p.getByRole("button", { name: "Amend", exact: true }).click();
  await settle(p, 300);
  check(
    "amending a commit the remote has asks for a second, deliberate click",
    (await p.getByText(/already on origin\. Amending rewrites it/).count()) ===
      1 &&
      (await p
        .getByRole("button", { name: "Amend anyway", exact: true })
        .count()) === 1,
  );

  await p.getByRole("button", { name: "Amend anyway", exact: true }).click();
  await settle(p, 600);
  await p.getByRole("button", { name: "Commits", exact: true }).click();
  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 600);

  await p
    .getByRole("textbox", { name: "Commit summary" })
    .fill("Fix the label");
  await p.getByRole("button", { name: "Commit", exact: true }).click();
  await settle(p, 700);
  await p.getByRole("button", { name: "Push", exact: true }).click();
  await settle(p, 700);

  await p.evaluate(() => window.__lynxMock.setBehind(2));
  await p.getByRole("button", { name: "Refresh", exact: true }).click();
  await settle(p, 500);
  check(
    "when the remote has commits and the branch has none of its own, the button is Pull",
    (await p.getByRole("button", { name: "Pull", exact: true }).count()) === 1,
  );
  await p.getByRole("button", { name: "Pull", exact: true }).click();
  await settle(p, 600);
  check(
    "pulling brings the branch level and says so",
    (await p.getByText("Pulled from origin.").count()) === 1 &&
      (await p.getByRole("button", { name: "Commit", exact: true }).count()) ===
        1,
  );

  // Something of its own to push, and the remote moved too: Sync.
  await p.getByRole("button", { name: "Stage README.md", exact: true }).click();
  await settle(p, 400);
  await p.getByRole("textbox", { name: "Commit summary" }).fill("Second fix");
  await p.getByRole("button", { name: "Commit", exact: true }).click();
  await settle(p, 700);
  await p.evaluate(() => window.__lynxMock.setBehind(1));
  await p.getByRole("button", { name: "Refresh", exact: true }).click();
  await settle(p, 500);
  check(
    "with commits on both sides the button is Sync, and says what each side has",
    (await p.getByRole("button", { name: "Sync", exact: true }).count()) ===
      1 && (await p.getByText(/1 to push, 1 to pull/).count()) === 1,
  );
  await p.getByRole("button", { name: "Sync", exact: true }).click();
  await settle(p, 800);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 600);

  await p.getByRole("switch", { name: "Amend last commit" }).click();
  await settle(p, 300);
  await p.getByRole("button", { name: "Amend", exact: true }).click();
  await settle(p, 200);
  await p.getByRole("button", { name: "Amend anyway", exact: true }).click();
  await settle(p, 700);

  const force = p.getByRole("button", { name: "Force push", exact: true });
  check(
    "after rewriting a pushed commit the button is Force push, and nothing else offers to sync",
    (await force.count()) === 1 &&
      (await p.getByRole("button", { name: "Sync", exact: true }).count()) ===
        0 &&
      (await p.getByText(/old copies of commits you rewrote/).count()) === 1,
  );
  await force.click();
  await settle(p, 300);
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
  await settle(p, 700);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 700);

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
  await settle(p, 500);
  check(
    "enabling auto-merge says GitHub will merge it once the requirements pass",
    (await p.getByText(/Auto-merge is on/).count()) === 1,
  );

  await p.getByRole("button", { name: "Convert to draft" }).click();
  await settle(p, 500);
  check(
    "converting to draft shows the Draft badge and offers Ready for review",
    (await p.getByText("Draft", { exact: true }).count()) >= 1 &&
      (await p.getByRole("button", { name: "Ready for review" }).count()) === 1,
  );
  await p.getByRole("button", { name: "Ready for review" }).click();
  await settle(p, 500);

  await p.getByRole("button", { name: "Close pull request" }).click();
  await settle(p, 500);
  check(
    "closing shows the Closed badge and offers to reopen",
    (await p.getByText("Closed", { exact: true }).count()) >= 1 &&
      (await p.getByRole("button", { name: "Reopen pull request" }).count()) ===
        1,
  );
  await p.getByRole("button", { name: "Reopen pull request" }).click();
  await settle(p, 500);

  await p.getByRole("button", { name: "Merge", exact: true }).click();
  await settle(p, 200);
  check(
    "a merge asks for a second click and says what it merges where",
    (await p.getByRole("button", { name: "Confirm merge" }).count()) === 1 &&
      (await p.getByText(/Merges #42 into main/).count()) === 1,
  );
  await p.getByRole("button", { name: "Confirm merge" }).click();
  await settle(p, 600);
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
  await settle(p, 300);
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
  await settle(p, 300);
  check(
    "cancelling keeps the worktree",
    (await p.getByRole("button", { name: /^feat\/auth/ }).count()) >= 1,
  );

  await p.getByRole("button", { name: "Clean up worktree" }).click();
  await settle(p, 300);
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Remove worktree and branch" })
    .click();
  await settle(p, 800);
  check(
    "confirming removes the worktree from the sidebar and says so",
    (await p.getByRole("button", { name: /^feat\/auth/ }).count()) === 0 &&
      (await p.getByText("feat/auth was cleaned up.").count()) === 1,
  );
});

// The Discussion view: the conversation, an inline thread to reply to and resolve, and a new comment.
await session("?mock=full", 1400, 800, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 700);

  check(
    "the pull request offers Checks and Discussion, with counts",
    (
      await p.getByRole("button", { name: /^Discussion/ }).textContent()
    )?.includes("3") === true,
  );
  await p.getByRole("button", { name: /^Discussion/ }).click();
  await settle(p, 400);
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
  await settle(p, 500);
  check(
    "replying adds the answer to the thread",
    (await p.getByText("It was a leftover, fixed.").count()) === 1,
  );

  await p.getByRole("button", { name: "Resolve thread" }).click();
  await settle(p, 400);
  check(
    "resolving marks the thread resolved and offers to reopen it",
    (await p.getByText("Resolved", { exact: true }).count()) === 1 &&
      (await p.getByRole("button", { name: "Reopen thread" }).count()) === 1,
  );

  await p
    .getByRole("textbox", { name: "Add a comment" })
    .fill("Thanks, looks good now.");
  await p.getByRole("button", { name: "Comment", exact: true }).click();
  await settle(p, 500);
  check(
    "a new comment joins the conversation and clears the box",
    (await p.getByText("Thanks, looks good now.").count()) === 1 &&
      (await p.getByRole("textbox", { name: "Add a comment" }).inputValue()) ===
        "",
  );

  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 700);

  check(
    "only a GitHub Actions job offers a log, and every check with a page offers Open",
    (await p.getByRole("button", { name: "Log of CI / test" }).count()) === 1 &&
      (await p.getByRole("button", { name: /^Log of / }).count()) === 1 &&
      (await p
        .getByRole("button", { name: "Open CI / test on GitHub" })
        .count()) === 1,
  );
  await p.getByRole("button", { name: "Log of CI / test" }).click();
  await settle(p, 400);
  const log = p.getByLabel("Log text of CI / test");
  check(
    "opening the log shows its end, says it was cut, and can be closed",
    ((await log.textContent()) ?? "").includes("test failed") &&
      (await p.getByText("Only the end of the log is shown.").count()) === 1,
  );
  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 500);
  const summary = p.getByRole("textbox", { name: "Commit summary" });
  const fix = p.getByRole("button", { name: "Fix with AI" });
  const failCommit = async () => {
    await p
      .getByRole("group", { name: "Changed files in the selected worktree" })
      .getByRole("button", { name: "Stage App.tsx" })
      .click();
    await settle(p, 300);
    await summary.fill("Ship it, hook fails");
    await p.getByRole("button", { name: "Commit", exact: true }).click();
    await settle(p, 500);
  };

  await failCommit();
  check(
    "a rejected commit shows git's words and no fix button while no agent is on",
    (await p.getByText("pre-commit hook failed").count()) === 1 &&
      (await fix.count()) === 0,
  );

  // Turn Claude Code on in Settings, then try again.
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await settle(p, 300);
  await p.getByRole("switch", { name: "Use Claude Code in Lynx" }).click();
  await settle(p, 300);
  await settle(p, 600);
  await p.keyboard.press("Escape");
  await settle(p, 300);

  await p.getByRole("button", { name: "Commit", exact: true }).click();
  await settle(p, 500);
  check(
    "with an agent on, a rejected commit offers Fix with AI",
    (await fix.count()) === 1,
  );

  await fix.click();
  await settle(p, 700);
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
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 500);
  await p.getByRole("tab", { name: "Pull request", exact: true }).click();
  await settle(p, 700);
  check(
    "Fix broken checks is not offered while no agent is on",
    (await p.getByRole("button", { name: "Fix broken checks" }).count()) === 0,
  );

  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Agents" }).click();
  await settle(p, 300);
  await p.getByRole("switch", { name: "Use Claude Code in Lynx" }).click();
  await settle(p, 300);
  await settle(p, 600);
  await p.keyboard.press("Escape");
  await settle(p, 300);

  await p.getByRole("button", { name: "Fix broken checks" }).click();
  await settle(p, 700);
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
  await settle(p, 300);
  check(
    "no notification while the window is in front",
    (await sent()).length === 0,
  );

  await inFront(false);
  await report("working");
  await settle(p, 200);
  check(
    "a working agent is not worth a notification",
    (await sent()).length === 0,
  );

  await report("done");
  await settle(p, 400);
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
  await settle(p, 200);
  check(
    "the same status twice does not notify twice",
    (await sent()).length === finished.length,
  );

  await report("needsYou");
  await settle(p, 300);
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
  await settle(p, 300);
  await p.getByRole("tab", { name: "Notifications", exact: true }).click();
  await settle(p, 300);
  const finishedSwitch = p.getByRole("switch", { name: "Agent finished" });
  check(
    "the Notifications page has both kinds, on by default",
    (await finishedSwitch.getAttribute("aria-checked")) === "true" &&
      (await p
        .getByRole("switch", { name: "Agent needs you" })
        .getAttribute("aria-checked")) === "true",
  );
  await settle(p, 300);
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
  await settle(p, 400);
  await p.keyboard.press("Escape");
  await settle(p, 300);

  await inFront(false);
  const before = (await sent()).length;
  await report("working");
  await report("done");
  await settle(p, 300);
  check(
    "with 'Agent finished' off, finishing is silent",
    (await sent()).length === before,
  );
  await report("needsYou");
  await settle(p, 300);
  check("and 'Agent needs you' still speaks", (await sent()).length > before);
});

// The file watcher: what an agent writes shows up in Files and Changes without leaving the window.
await session("?mock=full", 1400, 800, async (p) => {
  const worktree = "/home/dev/lynx-ide-worktrees/feat-auth";
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 400);
  await p.keyboard.press("Control+Shift+E");
  await settle(p, 600);

  check(
    "the new file is not listed before the agent writes it",
    (await p.getByRole("treeitem", { name: /^agent-notes\.md/ }).count()) === 0,
  );

  await p.evaluate((path) => {
    window.__lynxMock.agentWritesFile(path, "", "agent-notes.md");
  }, worktree);
  await settle(p, 500);
  check(
    "a file an agent writes appears in the explorer by itself",
    (await p.getByRole("treeitem", { name: /^agent-notes\.md/ }).count()) === 1,
  );

  await p.getByRole("tab", { name: "Changes", exact: true }).click();
  await settle(p, 500);
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
  await settle(p, 500);
  check(
    "a second file shows up in Changes too, with no reload",
    (await p
      .getByRole("tree", { name: /^Untracked files/ })
      .getByRole("treeitem", { name: /^plan\.md/ })
      .count()) === 1,
  );
});

// Plan limits in the footer: one button per plan, with the fullest window, and the details in a popover.
await session("?mock=full", 1400, 800, async (p) => {
  const footer = p.getByRole("contentinfo", { name: "Status bar" });
  const codex = footer.getByRole("button", { name: /^Codex:/ });
  check(
    "the footer shows the Codex plan with its share used and no other plan",
    (await codex.count()) === 1 &&
      ((await codex.innerText()) ?? "").includes("0%") &&
      (await footer.getByRole("group", { name: "Plan limits" }).count()) === 1,
  );

  await codex.click();
  await settle(p, 300);
  const details = p.getByRole("dialog");
  check(
    "its popover names the plan and window and says when it resets",
    ((await details.innerText()) ?? "").includes("Free") &&
      ((await details.innerText()) ?? "").includes("Monthly window") &&
      ((await details.innerText()) ?? "").includes("Resets in 29d"),
  );
  check(
    "the window's bar is a progress bar with its value, not only a colour",
    (await details
      .getByRole("progressbar", { name: "Monthly window" })
      .count()) === 1,
  );
  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the plan limits popover open",
    axe.length === 0,
    axe.join(" || "),
  );
  await p.keyboard.press("Escape");
  await settle(p, 200);
  check(
    "Escape closes the popover",
    (await p.getByRole("dialog").count()) === 0,
  );

  // A plan that is nearly out says so in words, and so does one that is not signed in.
  await p.evaluate(() => {
    const now = Math.floor(Date.now() / 1000);
    window.__lynxMock.setPlanUsage([
      {
        harness: "codex",
        plan: "plus",
        state: "ready",
        reason: null,
        observedAt: now,
        windows: [
          {
            kind: "session",
            minutes: 300,
            usedPercent: 40,
            resetsAt: now + 3600,
          },
          {
            kind: "weekly",
            minutes: 10080,
            usedPercent: 93,
            resetsAt: now + 86400,
          },
        ],
      },
      {
        harness: "claude",
        plan: null,
        state: "signInNeeded",
        reason: null,
        windows: [],
        observedAt: now,
      },
    ]);
    window.dispatchEvent(new Event("focus"));
  });
  await settle(p, 500);
  check(
    "the footer shows the fullest window, so a nearly used up plan reads 93%",
    (
      (await footer.getByRole("button", { name: /^Codex:/ }).innerText()) ?? ""
    ).includes("93%"),
  );
  check(
    "a plan that is not signed in says so",
    (await footer
      .getByRole("button", { name: /Sign in to Claude Code/ })
      .count()) === 1,
  );
  await footer.getByRole("button", { name: /^Codex:/ }).click();
  await settle(p, 300);
  check(
    "the popover lists both windows and flags the nearly used up one in words",
    (await p.getByRole("dialog").getByText("Almost used up").count()) === 1 &&
      (await p.getByRole("dialog").getByRole("progressbar").count()) === 2,
  );
});

// The plan limits settings: what shows in the footer, how it reads, and the warning levels that stay in order.
await session("?mock=full", 1400, 800, async (p) => {
  const footer = p.getByRole("contentinfo", { name: "Status bar" });
  const claude = footer.getByRole("button", { name: /^Claude Code:/ });
  const codex = footer.getByRole("button", { name: /^Codex:/ });
  // An open dialog hides the page behind it from role queries, so the footer is read by CSS meanwhile.
  const claudeBehind = p.locator("footer button", { hasText: "Claude Code" });
  const codexBehind = p.locator("footer button", { hasText: "Codex" });
  await p.evaluate(() => {
    const now = Math.floor(Date.now() / 1000);
    window.__lynxMock.setPlanUsage([
      {
        harness: "claude",
        plan: "pro",
        state: "ready",
        reason: null,
        observedAt: now,
        windows: [
          {
            kind: "session",
            minutes: 300,
            usedPercent: 82,
            resetsAt: now + 2 * 3600 + 900,
          },
          {
            kind: "weekly",
            minutes: 10080,
            usedPercent: 31,
            resetsAt: now + 4 * 86400,
          },
        ],
      },
      {
        harness: "codex",
        plan: "plus",
        state: "ready",
        reason: null,
        observedAt: now,
        windows: [
          {
            kind: "monthly",
            minutes: 43200,
            usedPercent: 12,
            resetsAt: now + 20 * 86400,
          },
        ],
      },
    ]);
    window.dispatchEvent(new Event("focus"));
  });
  await settle(p, 400);
  check(
    "both plans show in the footer by default, compact, with the countdown",
    (await claude.count()) === 1 &&
      (await codex.count()) === 1 &&
      ((await claude.textContent()) ?? "").includes("82% used") &&
      ((await claude.textContent()) ?? "").includes("2h 15m"),
  );

  await p.keyboard.press("Control+,");
  await settle(p, 300);
  // The limits are a tab of the Agents page.
  await p.getByRole("tab", { name: "Agents", exact: true }).click();
  await p.getByRole("tab", { name: "Plan limits", exact: true }).click();
  await settle(p, 300);
  const settings = p.locator('[role="dialog"]');
  const switchOn = async (name) =>
    (await settings
      .getByRole("switch", { name, exact: true })
      .getAttribute("aria-checked")) === "true";
  check(
    "the page offers the footer switch and one switch per plan, all on",
    (await switchOn("Show in the footer")) &&
      (await switchOn("Claude Code")) &&
      (await switchOn("Codex")),
  );
  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Plan limits page",
    axe.length === 0,
    axe.join(" || "),
  );

  await settings.getByRole("switch", { name: "Codex", exact: true }).click();
  await settle(p, 400);
  check(
    "turning a plan off takes it out of the footer",
    (await codexBehind.count()) === 0 && (await claudeBehind.count()) === 1,
  );
  await settings.getByRole("switch", { name: "Codex", exact: true }).click();
  await settle(p, 400);

  await settings.getByRole("combobox", { name: "Footer layout" }).click();
  await p.getByRole("option", { name: "Detailed", exact: true }).click();
  await settle(p, 300);
  await settings.getByRole("combobox", { name: "Percentages" }).click();
  await p.getByRole("option", { name: "Left", exact: true }).click();
  await settle(p, 400);
  const detailed = (await claudeBehind.textContent()) ?? "";
  check(
    "detailed layout names each window and reads what is left",
    detailed.includes("5h") &&
      detailed.includes("wk") &&
      detailed.includes("18% left") &&
      detailed.includes("69% left"),
    detailed,
  );

  // The warning levels cannot cross: the warning stops one below the critical level.
  await p.evaluate(() => window.__lynxMock.setUsageConfig({ warnAt: 89 }));
  await settle(p, 400);
  check(
    "the warning level cannot go up to the critical one",
    await settings
      .locator("#usage-warn-at")
      .locator("xpath=..")
      .getByRole("button", { name: "Increase" })
      .isDisabled(),
  );
  check(
    "the critical level cannot go down to the warning one",
    await settings
      .locator("#usage-critical-at")
      .locator("xpath=..")
      .getByRole("button", { name: "Decrease" })
      .isDisabled(),
  );

  await settings
    .getByRole("switch", { name: "Show plan names", exact: true })
    .click();
  await settle(p, 400);
  const marks = p.locator('footer [role="group"] button');
  check(
    "with names off the footer keeps each plan's number but not its name, which stays in the label",
    (await marks.count()) === 2 &&
      !((await marks.first().textContent()) ?? "").includes("Claude") &&
      ((await marks.first().getAttribute("aria-label")) ?? "").includes(
        "Claude Code",
      ),
  );

  await p.keyboard.press("Escape");
  await settle(p, 300);
  await p.evaluate(() => window.__lynxMock.setUsageConfig({ visible: false }));
  await settle(p, 400);
  check(
    "switching the footer off hides every plan",
    (await footer.getByRole("group", { name: "Plan limits" }).count()) === 0,
  );
});

// Updates: a check finds a new version, the app says so, and the page installs it and offers a
// restart that asks first.
await session("?mock=full", 1400, 800, async (p) => {
  await p.keyboard.press("Control+,");
  await settle(p, 300);
  await p.getByRole("tab", { name: "Updates", exact: true }).click();
  await settle(p, 300);
  check(
    "the Updates page shows the installed version and a check button",
    (await p.getByText("Installed version 0.1.0").count()) === 1 &&
      (await p.getByRole("button", { name: "Check now" }).count()) === 1,
  );

  await p.getByRole("button", { name: "Check now" }).click();
  await p.waitForTimeout(1500);
  check(
    "a check finds 0.1.1, offers it on the page, and tells the user in the app",
    (await p.getByRole("button", { name: "Update to 0.1.1" }).count()) === 1 &&
      (await p.getByText("Lynx 0.1.1 is available").count()) >= 1,
  );

  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Updates page",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByRole("button", { name: "Update to 0.1.1" }).click();
  await p.waitForTimeout(1800);
  check(
    "an install ends with a restart offer",
    (await p
      .getByRole("button", { name: "Restart Lynx", exact: true })
      .count()) === 1,
  );

  await p.getByRole("button", { name: "Restart Lynx", exact: true }).click();
  await settle(p, 300);
  check(
    "restarting asks first, and says that terminals and agents close",
    (await p.getByRole("alertdialog").count()) === 1 &&
      (await p.getByText("Open terminals and agents will close").count()) === 1,
  );
  await p.keyboard.press("Escape");
});

// Bots: the ChatGPT sign-in waits for the browser, comes back signed in, and signing out forgets
// the account. The plan switch starts off and saves when it is turned on.
await session("?mock=full", 1400, 800, async (p) => {
  await p.keyboard.press("Control+,");
  await p.waitForTimeout(300);
  await p.getByRole("tab", { name: "Bots", exact: true }).click();
  await p.waitForTimeout(300);
  const signIn = p.getByRole("button", {
    name: "Sign in with ChatGPT",
    exact: true,
  });
  const plan = p.getByRole("switch", { name: "Use the ChatGPT plan" });
  check(
    "the Bots page offers the ChatGPT sign-in, and the plan starts off",
    (await signIn.count()) === 1 && !(await plan.isChecked()),
  );

  await signIn.click();
  await p.waitForTimeout(300);
  check(
    "signing in waits for the browser",
    (await p.getByText("Waiting for the sign-in in your browser…").count()) ===
      1,
  );

  await p.waitForTimeout(1400);
  check(
    "the answer comes back signed in as the account",
    (await p.getByText("Signed in as dev@example.com.").count()) === 1 &&
      (await p
        .getByRole("button", { name: "Sign out", exact: true })
        .count()) === 1,
  );

  // Jev picks who answers in a channel. It starts off, with no key, and the key is only ever written.
  const jev = p.getByRole("switch", { name: "Let Jev choose who answers" });
  check(
    "Jev starts off with no key saved",
    !(await jev.isChecked()) &&
      (await p.getByText("No key saved.", { exact: true }).count()) === 1,
  );
  await p.getByLabel("Jev API key", { exact: true }).fill("jev_secret");
  await p.getByRole("button", { name: "Save key", exact: true }).click();
  await p
    .getByText("A key is saved. It is never shown again.", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "a saved key is never shown back",
    (await p.getByLabel("Jev API key", { exact: true }).inputValue()) === "",
  );
  await p.getByRole("button", { name: "Test", exact: true }).click();
  await p
    .getByText("Jev answered in 340 ms.", { exact: true })
    .waitFor({ timeout: 5000 });
  check("the test says how long Jev took", true);
  await jev.click();
  await p.waitForTimeout(300);
  check("Jev can be turned on once a key is saved", await jev.isChecked());

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the Bots page",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByRole("button", { name: "Sign out", exact: true }).click();
  await p.waitForTimeout(300);
  check(
    "signing out forgets the account",
    (await p.getByText("Not signed in.").count()) === 1,
  );

  await plan.click();
  await p.waitForTimeout(300);
  check("turning the plan on sticks", await plan.isChecked());
});

// Bots: with no bot the Bots view says so and offers the wizard. The wizard makes a bot in steps, and
// the new bot is picked, so the main area becomes its chat: a reply comes back after the bot's status
// says it is thinking. The side panel then shows the bot's own tabs, not the worktree's.
await session("?mock=empty", 1400, 800, async (p) => {
  await p.getByRole("button", { name: "Bots", exact: true }).click();
  await p.waitForTimeout(400);
  check(
    "with no bot yet, the Bots view says so and the main area asks for a pick",
    (await p.getByText("No bots yet.", { exact: true }).count()) === 1 &&
      (await p
        .getByText("Pick a bot in the list to talk to it.", { exact: true })
        .count()) === 1,
  );

  await p.getByRole("button", { name: "New bot", exact: true }).click();
  const wizard = p.getByRole("dialog", { name: "Create a bot", exact: true });
  await wizard.waitFor();
  const next = wizard.getByRole("button", { name: "Next", exact: true });
  check(
    "the wizard starts on the name step, and Next waits for a name",
    (await wizard.getByText("Step 1 of 4", { exact: true }).count()) === 1 &&
      (await next.isDisabled()),
  );
  await wizard
    .getByRole("textbox", { name: "Name", exact: true })
    .fill("Bento");
  await wizard
    .getByRole("textbox", { name: "Team", exact: true })
    .fill("infra");
  await wizard.getByRole("button", { name: "Sky", exact: true }).click();
  await next.click();

  check(
    "the engine step offers Claude, and the other engines wait",
    (await wizard
      .getByRole("radio", { name: "Claude (CLI)", exact: true })
      .getAttribute("aria-checked")) === "true" &&
      (await wizard.getByRole("radio", { name: /ChatGPT/ }).isDisabled()),
  );
  await next.click();

  // The label wraps a hint, so its accessible name starts with "Instructions".
  await wizard
    .getByRole("textbox", { name: /^Instructions/ })
    .fill("Você organiza a rotina.");
  await next.click();
  check(
    "the review shows what the bot will be before it is added",
    (await wizard.getByText("Step 4 of 4", { exact: true }).count()) === 1 &&
      (await wizard
        .getByText("Você organiza a rotina.", { exact: true })
        .count()) === 1,
  );
  await wizard
    .getByRole("button", { name: "Add to the team", exact: true })
    .click();
  await wizard.waitFor({ state: "detached" });

  const message = p.getByRole("textbox", {
    name: "Message Bento",
    exact: true,
  });
  check(
    "the new bot is listed and picked, so the main area is its chat",
    (await p
      .getByRole("list", { name: "Bots" })
      .getByRole("button", { name: /Bento/ })
      .getAttribute("aria-current")) === "true" &&
      (await message.count()) === 1,
  );

  check(
    "the bot's team shows beside it in the Agents list",
    (await p
      .getByRole("list", { name: "Bots" })
      .getByText("infra", { exact: true })
      .count()) === 1,
  );
  await message.fill("Oi, tudo bem?");
  await message.press("Enter");
  const working = p.getByRole("status").getByText("Working…", { exact: true });
  await working.waitFor({ timeout: 1000 });
  check(
    "while the bot has not written anything yet, its reply row says it is working",
    await working.isVisible(),
  );
  await p
    .getByText("Oi! Sou Bento.", { exact: false })
    .first()
    .waitFor({ timeout: 2000 });
  check(
    "the reply is shown as it is written: the first words arrive before the end, and Working gives way",
    (await working.count()) === 0 &&
      (await p.getByText("Revisar o PR").count()) === 0,
  );
  await p.getByText("Revisar o PR").waitFor({ timeout: 5000 });
  check(
    "the finished reply stays in the conversation, once the status is gone",
    (await p.locator('[role="log"]').getByRole("status").count()) === 0 &&
      (await p.getByText("Oi! Sou Bento.", { exact: false }).count()) >= 1,
  );

  const bold = await p.locator("strong", { hasText: "Hoje:" }).count();
  const items = await p.locator("li", { hasText: "Revisar o PR" }).count();
  const shown = await p.locator('[role="log"]').innerText();
  check(
    "markdown in a reply is rendered, not shown as its symbols",
    bold === 1 && items === 1 && !shown.includes("**"),
  );

  const first = await p.locator('[role="log"] > article').first().boundingBox();
  const last = await p.locator('[role="log"] > article').last().boundingBox();
  const area = await p
    .locator('[role="log"]')
    .locator("xpath=../..")
    .boundingBox();
  const box = await message.boundingBox();
  check(
    "the conversation starts from the bottom: the newest message sits just above the box to write in",
    first !== null &&
      last !== null &&
      area !== null &&
      box !== null &&
      box.y - (last.y + last.height) < 80 &&
      first.y - area.y > 150,
  );

  // A bot asks to read a registered repository: the request is a card in the chat. Approving it grants the
  // read, and the bot answers in the same conversation.
  await message.fill("preciso de acesso ao lynx");
  await message.press("Enter");
  const approve = p.getByRole("button", { name: "Approve", exact: true });
  await approve.waitFor({ timeout: 3000 });
  check(
    "a request to read a repository is a card with approve and refuse",
    (await p
      .getByText("Wants to read this project", { exact: true })
      .count()) === 1 &&
      (await p
        .getByRole("listitem")
        .filter({ hasText: /^lynx$/ })
        .count()) === 1 &&
      (await p.getByRole("button", { name: "Refuse", exact: true }).count()) ===
        1,
  );
  await approve.click();
  await p
    .getByText("Obrigado! Vou usar lynx.", { exact: true })
    .waitFor({ timeout: 3000 });
  check(
    "approving grants the read, and the bot answers in the same conversation",
    (await p.getByText("Access granted", { exact: true }).count()) === 1 &&
      (await p
        .getByRole("button", { name: "Approve", exact: true })
        .count()) === 0,
  );

  // A block of code in a reply names its language and copies by a button; a whole message copies too.
  await p.evaluate(() => {
    window.__copied = [];
    navigator.clipboard.writeText = async (text) => {
      window.__copied.push(text);
    };
  });
  await message.fill("mostra um comando");
  await message.press("Enter");
  await p
    .getByText("Depois abra o navegador.", { exact: true })
    .waitFor({ timeout: 3000 });
  check(
    "a block of code in a reply names its language",
    (await p.getByText("bash", { exact: true }).count()) === 1,
  );
  await p.getByRole("button", { name: "Copy code", exact: true }).click();
  check(
    "the copy button copies the block's code, and says it did",
    (await p.evaluate(() => window.__copied)).join("|") ===
      "minha-cli serve --port 8080" &&
      (await p.getByRole("button", { name: "Copied", exact: true }).count()) ===
        1,
    JSON.stringify(await p.evaluate(() => window.__copied)),
  );
  await p
    .getByRole("button", { name: "Copy message", exact: true })
    .last()
    .click();
  const copied = await p.evaluate(() => window.__copied);
  check(
    "a message copies as the text the bot wrote",
    copied.length === 2 &&
      copied[1].startsWith("Rode isto:") &&
      copied[1].includes("```bash\nminha-cli serve --port 8080\n```"),
    JSON.stringify(copied),
  );
  await p.evaluate(() => delete navigator.clipboard.writeText);

  // The same card carries any other request: here the bot asks to run command patterns.
  await message.fill("preciso rodar a cli");
  await message.press("Enter");
  await p
    .getByText("Wants to run these commands", { exact: true })
    .waitFor({ timeout: 3000 });
  check(
    "a request to run commands is the same card, listing each pattern",
    (await p
      .getByRole("listitem")
      .filter({ hasText: /^minha-cli \*$/ })
      .count()) === 1 &&
      (await p
        .getByRole("listitem")
        .filter({ hasText: /^git status$/ })
        .count()) === 1,
  );
  await p.getByRole("button", { name: "Approve", exact: true }).click();
  await p
    .getByText("Obrigado! Vou usar minha-cli *, git status.", { exact: true })
    .waitFor({ timeout: 3000 });
  check("approving the commands keeps the answer in the card", true);

  // Starting agents is the same card again, for one project, and says they run without asking.
  await message.fill("preciso lançar um agente");
  await message.press("Enter");
  await p
    .getByText(
      "Wants to start agents in this project. They run without asking for permission",
      { exact: true },
    )
    .waitFor({ timeout: 3000 });
  await p.getByRole("button", { name: "Approve", exact: true }).click();
  await p
    .getByText("Certo! Vou lançar agentes em lynx.", { exact: true })
    .waitFor({ timeout: 3000 });
  check(
    "a request to start agents is the same card, and approving it is kept",
    true,
  );

  await p
    .getByRole("button", { name: "Toggle side panel", exact: true })
    .click();
  const detailsTab = p.getByRole("tab", { name: "Bot details", exact: true });
  // The side panel's code loads the first time it opens, so its tabs can come a moment later.
  await detailsTab.waitFor({ timeout: 5000 }).catch(() => {});
  const browserTab = p.getByRole("tab", { name: "Bot browser", exact: true });
  check(
    "the side panel opens on the bot's tabs, and Files is not one of them",
    (await detailsTab.count()) === 1 &&
      (await browserTab.count()) === 1 &&
      (await p.getByRole("tab", { name: "Files", exact: true }).count()) === 0,
    `details ${await detailsTab.count()}, browser ${await browserTab.count()}, files ${await p.getByRole("tab", { name: "Files", exact: true }).count()}`,
  );
  await p
    .getByText("Você organiza a rotina.", { exact: true })
    .last()
    .waitFor();
  check("the details tab shows the instructions the bot was made with", true);

  // The approved patterns are in the bot's list, and the person can add and refuse patterns there too.
  await detailsTab.click();
  const commandList = p.getByRole("list", { name: "Commands", exact: true });
  await commandList
    .getByText("minha-cli *", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "the patterns the person approved are in the bot's commands",
    (await commandList.getByText("git status", { exact: true }).count()) === 1,
  );
  await p.getByRole("button", { name: "Add commands", exact: true }).click();
  const patterns = p.getByRole("textbox", { name: "Patterns", exact: true });
  const addPatterns = p
    .getByRole("dialog")
    .getByRole("button", { name: "Add", exact: true });
  await patterns.fill("bash *");
  await addPatterns.click();
  await p.getByRole("alert").waitFor({ timeout: 5000 });
  check(
    "a pattern that would allow everything is refused in the dialog",
    (await p.getByRole("dialog").count()) === 1 &&
      (await p.getByRole("alert").innerText()).includes("not allowed"),
  );
  await patterns.fill("git log *");
  await addPatterns.click();
  await commandList
    .getByText("git log *", { exact: true })
    .waitFor({ timeout: 5000 });
  await p
    .getByRole("button", { name: "Remove git log *", exact: true })
    .click();
  await p.waitForTimeout(300);
  check(
    "a pattern added by hand can be taken back",
    (await commandList.getByText("git log *", { exact: true }).count()) === 0,
  );

  // A command the bot leaves running is in its Agents tab, with the end of its output, and can be stopped.
  await p.getByRole("tab", { name: "Agents", exact: true }).click();
  check(
    "with nothing left running, the Agents tab says so",
    (await p.getByText("No commands running.", { exact: false }).count()) === 1,
  );
  await message.fill("deixa rodando");
  await message.press("Enter");
  await p
    .getByText("Iniciei minha-cli serve.", { exact: true })
    .waitFor({ timeout: 3000 });
  await p.waitForTimeout(3500);
  const left = p.getByRole("list", { name: "Commands", exact: true });
  await left
    .getByText("minha-cli serve", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "a command left running is listed as running, with what it printed",
    (await left.getByText("Running", { exact: true }).count()) === 1 &&
      (await left.getByText("listening on 8080", { exact: false }).count()) ===
        1,
  );
  await p
    .getByRole("button", { name: "Stop minha-cli serve", exact: true })
    .click();
  await left.getByText("Ended", { exact: true }).waitFor({ timeout: 5000 });
  check(
    "stopping it ends it, and the stop button goes",
    (await p
      .getByRole("button", { name: "Stop minha-cli serve", exact: true })
      .count()) === 0,
  );
  await p.getByRole("tab", { name: "Bot details", exact: true }).click();

  // A bot's threads are separate conversations under it: the first message titles one, a new one
  // starts empty, and each keeps what was said in it.
  const threadList = p.getByRole("list", { name: "Threads", exact: true });
  await threadList
    .getByText("Oi, tudo bem?", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "the thread the bot's chat started in is titled by its first message",
    (await threadList.getByRole("listitem").count()) === 1,
  );
  await p.getByRole("button", { name: "New thread", exact: true }).click();
  await threadList
    .getByText("New thread", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "a new thread starts empty, untitled, and is the one shown",
    (await threadList.getByRole("listitem").count()) === 2 &&
      (await p
        .getByText("Depois abra o navegador.", { exact: true })
        .count()) === 0,
  );
  await message.fill("oi de novo");
  await message.press("Enter");
  await threadList
    .getByText("oi de novo", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "the first message titles the new thread",
    (await threadList.getByText("New thread", { exact: true }).count()) === 0,
  );
  await threadList.getByText("Oi, tudo bem?", { exact: true }).click();
  await p
    .getByText("Depois abra o navegador.", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "going back to the first thread shows what was said in it, and not the other's",
    (await p.getByText("oi de novo", { exact: true }).count()) === 0 ||
      (await p
        .locator('[role="log"]')
        .getByText("oi de novo", { exact: true })
        .count()) === 0,
  );

  // The column can be searched and put away; the choice is the person's.
  const search = p.getByRole("textbox", {
    name: "Search threads",
    exact: true,
  });
  await search.fill("zzz");
  await p
    .getByText("No thread matches.", { exact: true })
    .waitFor({ timeout: 3000 });
  await search.fill("oi de");
  check(
    "searching narrows the list to the threads that match",
    (await threadList.getByRole("listitem").count()) === 1,
  );
  await search.fill("");
  await p.getByRole("button", { name: "Hide threads", exact: true }).click();
  await p.waitForTimeout(200);
  check(
    "the column of threads can be hidden, and shown again",
    (await p
      .getByRole("textbox", { name: "Search threads", exact: true })
      .count()) === 0,
  );
  await p.getByRole("button", { name: "Show threads", exact: true }).click();
  await threadList
    .getByText("Oi, tudo bem?", { exact: true })
    .waitFor({ timeout: 3000 });

  // Renaming and deleting are in the row's menu.
  const second = threadList
    .getByRole("listitem")
    .filter({ hasText: "oi de novo" });
  await second.getByRole("button", { name: /Rename/ }).click();
  await p.getByRole("menuitem", { name: "Rename", exact: true }).click();
  await p.getByRole("textbox", { name: "Title", exact: true }).fill("Revisão");
  await p.getByRole("button", { name: "Save", exact: true }).click();
  await threadList
    .getByText("Revisão", { exact: true })
    .waitFor({ timeout: 5000 });
  check("a thread can be renamed", true);
  await threadList
    .getByRole("listitem")
    .filter({ hasText: "Revisão" })
    .getByRole("button", { name: /Rename/ })
    .click();
  await p.getByRole("menuitem", { name: "Delete", exact: true }).click();
  await p
    .getByRole("alertdialog")
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await p.waitForTimeout(400);
  check(
    "deleting a thread asks first and leaves the other",
    (await threadList.getByRole("listitem").count()) === 1 &&
      (await threadList.getByText("Oi, tudo bem?", { exact: true }).count()) ===
        1,
  );

  // An answer being written can be stopped: what was written stays, marked as stopped.
  await message.fill("demora");
  await message.press("Enter");
  const stop = p.getByRole("button", { name: "Stop", exact: true });
  await stop.waitFor({ timeout: 3000 });
  check(
    "while the bot writes, the box offers Stop and not Send",
    (await p.getByRole("button", { name: "Send", exact: true }).count()) === 0,
  );
  // Another thread of the same bot is free to talk while this one writes.
  await p.getByRole("button", { name: "New thread", exact: true }).click();
  await p.waitForTimeout(300);
  check(
    "another thread of the bot can be written in while one answers",
    await message.isEnabled(),
  );
  await threadList.getByText("Oi, tudo bem?", { exact: true }).click();
  await stop.waitFor({ timeout: 3000 });
  await stop.click();
  await p.getByText("Stopped", { exact: true }).waitFor({ timeout: 5000 });
  check(
    "stopping keeps what was written, says it was stopped, and brings Send back",
    (await p.getByText("Começando a escrever", { exact: false }).count()) >=
      1 &&
      (await p.getByRole("button", { name: "Send", exact: true }).count()) ===
        1,
  );

  // Where the bot may start agents is listed with the access it was given, and can be taken back.
  const startList = p.getByRole("list", {
    name: "Projects where it can start agents",
    exact: true,
  });
  await startList.getByText("lynx", { exact: true }).waitFor({ timeout: 5000 });
  await p
    .getByRole("button", { name: "Remove agents in lynx", exact: true })
    .click();
  await p
    .getByText("No projects yet.", { exact: true })
    .last()
    .waitFor({ timeout: 5000 });
  check(
    "the project where it may start agents is listed, and taking it back empties the list",
    (await startList.count()) === 0,
  );

  // Skills and MCP servers are added from the details tab. The mock's folder picker returns a fixed folder.
  await detailsTab.click();
  await p.getByRole("button", { name: "Add skill", exact: true }).click();
  const skills = p.getByRole("list", { name: "Skills", exact: true });
  await skills.getByText("web-app", { exact: true }).waitFor({ timeout: 5000 });
  check(
    "a skill copied in from a folder is listed under the bot",
    (await skills.getByText("web-app", { exact: true }).count()) === 1,
  );
  await p.getByRole("button", { name: "Add servers", exact: true }).click();
  const config = p.getByRole("textbox", { name: "Config", exact: true });
  const addServers = p
    .getByRole("dialog")
    .getByRole("button", { name: "Add", exact: true });
  await config.fill("não é json");
  await addServers.click();
  await p.getByRole("alert").waitFor({ timeout: 5000 });
  check(
    "a config Lynx cannot read is refused in the dialog, which stays open",
    (await p.getByRole("dialog").count()) === 1 &&
      (await p.getByRole("alert").innerText()).includes("not valid JSON"),
  );
  await config.fill(
    JSON.stringify({
      mcpServers: { docs: { type: "http", url: "https://example.com/mcp" } },
    }),
  );
  await addServers.click();
  const servers = p.getByRole("list", { name: "MCP servers", exact: true });
  await servers.getByText("docs", { exact: true }).waitFor({ timeout: 5000 });
  check(
    "an MCP server added from a config is listed with the address it is called at",
    (await p.getByRole("dialog").count()) === 0 &&
      (await servers
        .getByText("Calls https://example.com/mcp", { exact: true })
        .count()) === 1,
  );
  await p.getByRole("button", { name: "Remove docs", exact: true }).click();
  await p
    .getByText("No MCP servers yet.", { exact: true })
    .waitFor({ timeout: 5000 });
  await p.getByRole("button", { name: "Remove web-app", exact: true }).click();
  await p
    .getByText("No skills yet.", { exact: true })
    .waitFor({ timeout: 5000 });
  check("a removed skill and server leave the bot's lists", true);

  await browserTab.click();
  check(
    "the browser tab says whose browser it is, and that the bot cannot use it yet",
    (await p.getByText("Bento's browser", { exact: true }).count()) === 1,
  );

  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations on the bot chat and its side panel",
    axe.length === 0,
    axe.join(" || "),
  );

  await p.getByRole("button", { name: "New bot", exact: true }).click();
  await wizard.waitFor();
  // The dialog fades in; axe must not measure the colours while it is still transparent.
  await p.waitForTimeout(400);
  const wizardAxe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations in the bot wizard",
    wizardAxe.length === 0,
    wizardAxe.join(" || "),
  );
  await p.keyboard.press("Escape");
  await wizard.waitFor({ state: "detached" });
  // Channels sit outside the teams: a channel is made from the sidebar, with the bots that sit in it.
  await p.getByRole("button", { name: "New channel", exact: true }).click();
  const channelDialog = p.getByRole("dialog", {
    name: "New channel",
    exact: true,
  });
  await channelDialog.waitFor();
  await channelDialog
    .getByRole("textbox", { name: "Name", exact: true })
    .fill("engenharia");
  await channelDialog
    .getByRole("button", { name: "Bento", exact: true })
    .click();
  await channelDialog
    .getByRole("button", { name: "Create channel", exact: true })
    .click();
  await channelDialog.waitFor({ state: "detached" });
  check(
    "a new channel is listed in the sidebar and picked, so the main area is its conversation",
    (await p
      .getByRole("list", { name: "Channels" })
      .getByRole("button", { name: /engenharia/ })
      .getAttribute("aria-current")) === "true" &&
      (await p
        .getByRole("textbox", { name: "Message #engenharia", exact: true })
        .count()) === 1,
  );
  const channelLog = p.getByRole("log", {
    name: "Conversation in #engenharia",
    exact: true,
  });
  await p
    .getByRole("textbox", { name: "Message #engenharia", exact: true })
    .fill("Oi pessoal");
  await p
    .getByRole("textbox", { name: "Message #engenharia", exact: true })
    .press("Enter");
  const writing = p
    .getByRole("status")
    .getByText("Bento is writing…", { exact: true });
  await writing.waitFor({ timeout: 3000 });
  check(
    "while Bento writes, the status line names him",
    await writing.isVisible(),
  );
  const channelReply = channelLog.getByText("Oi, sou Bento.", { exact: false });
  await channelReply.waitFor({ timeout: 5000 });
  check(
    "a member's reply in the channel is streamed under the name of the bot that wrote it",
    (await channelLog.getByText("Bento", { exact: true }).count()) === 1 &&
      (await channelReply.count()) === 1,
  );

  // With a channel picked, the details tab shows the channel and the bots in it.
  await p.getByRole("tab", { name: "Bot details", exact: true }).click();
  check(
    "with a channel picked, the details tab lists its members",
    (await p
      .getByRole("list", { name: "Members", exact: true })
      .getByText("Bento", { exact: true })
      .count()) === 1 &&
      (await p.getByRole("tabpanel").getByText("engenharia").count()) >= 1,
  );

  // The members of the channel change from its details, and go back in the same way.
  await p.getByRole("button", { name: "Edit members", exact: true }).click();
  await p
    .getByRole("tabpanel")
    .getByRole("button", { name: "Bento", exact: true })
    .click();
  await p.getByRole("button", { name: "Save", exact: true }).click();
  check(
    "saving the members without Bento leaves the channel with only its person",
    (await p
      .getByText("Only you, until a bot joins.", { exact: true })
      .count()) === 1,
  );
  await p.getByRole("button", { name: "Edit members", exact: true }).click();
  await p
    .getByRole("tabpanel")
    .getByRole("button", { name: "Bento", exact: true })
    .click();
  await p.getByRole("button", { name: "Save", exact: true }).click();
  check(
    "adding Bento back puts it among the channel's members",
    (await p
      .getByRole("list", { name: "Members", exact: true })
      .getByText("Bento", { exact: true })
      .count()) === 1,
  );

  // Back to the bot's own chat, which the checks below read.
  await p
    .getByRole("list", { name: "Bots" })
    .getByRole("button", { name: /Bento/ })
    .click();
  // A reply that does not come stays in the conversation as an error line.
  await message.fill("falhar agora");
  await message.press("Enter");
  check(
    "a reply that does not come stays in the conversation as an error line",
    (await p
      .getByText("The answer did not come: Claude did not answer.", {
        exact: true,
      })
      .count()) === 1,
  );

  // An https link in a reply opens in the browser, and the app's own window stays where it is.
  const link = p.getByRole("link", { name: "a agenda", exact: true }).first();
  await link.waitFor({ timeout: 5000 });
  const before = p.url();
  await link.click();
  check(
    "a link in a reply is an https link, and clicking it leaves the app's window where it is",
    (await link.getAttribute("href")) === "https://example.com/agenda" &&
      p.url() === before,
  );

  // The details tab edits the instructions, and deletes the bot after a confirmation.
  await p.getByRole("tab", { name: "Bot details", exact: true }).click();
  await p.getByRole("button", { name: "Edit", exact: true }).click();
  const editor = p.getByRole("textbox", { name: "Instructions", exact: true });
  await editor.fill("Organize o dia com calma.");
  await p.getByRole("button", { name: "Save", exact: true }).click();
  await editor.waitFor({ state: "detached" });
  check(
    "saving new instructions shows them in the details tab",
    (await p
      .getByText("Organize o dia com calma.", { exact: true })
      .count()) === 1,
  );

  // Engine, model and team change together, and the team shows where the bot is listed.
  await p
    .getByRole("button", { name: "Edit engine, model and team", exact: true })
    .click();
  await p.getByRole("textbox", { name: "Team", exact: true }).fill("dados");
  await p.getByRole("button", { name: "Save", exact: true }).click();
  check(
    "saving the engine, model and team shows the new team in the details and in the list",
    (await p
      .getByRole("tabpanel")
      .getByText("dados", { exact: true })
      .count()) === 1 &&
      (await p
        .getByRole("list", { name: "Bots" })
        .getByText("dados", { exact: true })
        .count()) === 1,
  );

  await p.getByRole("button", { name: "Delete bot", exact: true }).click();
  const dialog = p.getByRole("alertdialog");
  await dialog.waitFor();
  check(
    "deleting asks first, in a dialog that names the bot",
    (await dialog.getByText("Delete Bento?", { exact: true }).count()) === 1,
  );
  await p.evaluate(axeSource);
  // The dialog fades in; axe must not measure the colours while it is still transparent.
  await p.waitForTimeout(400);
  const deleteAxe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations in the delete confirmation",
    deleteAxe.length === 0,
    deleteAxe.join(" || "),
  );
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  await p.getByText("No bots yet.", { exact: true }).waitFor({ timeout: 5000 });
  check(
    "the bot is gone, and the Bots view asks for one again",
    (await p.getByRole("list", { name: "Bots" }).count()) === 0 &&
      (await p
        .getByText("Pick a bot in the list to talk to it.", { exact: true })
        .count()) >= 1,
  );

  // A channel is removed from its details tab, after a confirmation that names it.
  await p
    .getByRole("list", { name: "Channels" })
    .getByRole("button", { name: /engenharia/ })
    .click();
  await p.getByRole("tab", { name: "Bot details", exact: true }).click();
  await p.getByRole("button", { name: "Delete channel", exact: true }).click();
  const deleteDialog = p.getByRole("alertdialog");
  await deleteDialog.waitFor();
  check(
    "deleting a channel asks first, in a dialog that names it",
    (await deleteDialog
      .getByText("Delete #engenharia?", { exact: true })
      .count()) === 1,
  );
  await deleteDialog
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await p
    .getByText("No channels yet.", { exact: true })
    .waitFor({ timeout: 5000 });
  check(
    "the channel is gone from the sidebar, and no channel is picked",
    (await p
      .getByRole("list", { name: "Channels" })
      .getByRole("button", { name: /engenharia/ })
      .count()) === 0 &&
      (await p
        .getByText("Say something in #engenharia.", { exact: true })
        .count()) === 0,
  );
});

// The team is a prototype of a coordinator's run, so only the mock offers it. The org chart draws
// the run, a member's panel opens over it, and a member's terminal dialog covers the central area.
const teamEdges = (p) => p.locator(".react-flow__edge").count();
const teamNodes = (p) => p.locator(".react-flow__node").count();
// The transform of a card on the map, in flow pixels.
const cardAt = (p, id) =>
  p.evaluate((id) => {
    const node = document.querySelector(`.react-flow__node[data-id="${id}"]`);
    const m = /translate\(([-\d.]+)px,\s*([-\d.]+)px\)/.exec(
      node?.style.transform ?? "",
    );
    return m ? { x: Number(m[1]), y: Number(m[2]) } : null;
  }, id);
// How much the map is zoomed: a drag of one row on screen is ROW * zoom pixels.
const mapZoom = (p) =>
  p.evaluate(() => {
    const viewport = document.querySelector(".react-flow__viewport");
    return new DOMMatrix(getComputedStyle(viewport).transform).a;
  });
// Selects a pair line by its hit path (the tree's lines are not selectable).
const clickEdge = (p, id) =>
  p.evaluate((id) => {
    const path = document.querySelector(
      `.react-flow__edge[data-id="${id}"] .react-flow__edge-interaction`,
    );
    path?.dispatchEvent(
      new MouseEvent("click", {
        bubbles: true,
        cancelable: true,
        view: window,
      }),
    );
  }, id);
// Opens a submenu (Radix opens it on click, or on the arrow key from its trigger).
const openSubmenu = async (p, name) => {
  const trigger = p.getByRole("menuitem", { name, exact: true });
  await trigger.click();
  await settle(p, 300);
  if ((await p.getByRole("menuitemcheckbox").count()) === 0) {
    await trigger.focus();
    await p.keyboard.press("ArrowRight");
    await settle(p, 300);
  }
};

await session("?mock=full", 1400, 860, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 300);
  check(
    "the sidebar header has the Terminal | Team switch and the All | Active filter",
    (await p
      .getByRole("group", { name: "Central view", exact: true })
      .count()) === 1 &&
      (await p
        .getByRole("group", { name: "Which projects", exact: true })
        .count()) === 1,
  );
  await p.getByRole("button", { name: "Team", exact: true }).click();
  // The team view loads on demand: wait for the map, not a fixed time.
  await p.locator(".react-flow__node").first().waitFor({ timeout: 15000 });
  await settle(p, 600);
  check(
    "Team puts the team bar in place of the terminal tabs",
    (await p.getByRole("tablist", { name: "Terminals" }).count()) === 0 &&
      (await p.getByText("Auth refactor", { exact: true }).count()) === 1,
  );
  check(
    "the org chart draws the five members, joined by four lines (three tree lines and the Builder-Reviewer pair)",
    (await teamNodes(p)) === 5 && (await teamEdges(p)) === 4,
    `${await teamNodes(p)} nodes, ${await teamEdges(p)} edges`,
  );
  check(
    "the member that needs the user opens in a panel, which shows the question and no Reply",
    (await p
      .getByRole("dialog", { name: "Researcher · OpenCode", exact: true })
      .count()) === 1 &&
      (await p.getByText("which OAuth lib?", { exact: true }).count()) === 1 &&
      (await p.getByRole("button", { name: "Reply", exact: true }).count()) ===
        0,
  );

  await p.keyboard.press("Escape");
  await settle(p, 200);
  check(
    "Escape closes the panel, and the focus goes back to the member's card",
    (await p.getByRole("dialog").count()) === 0 &&
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-team-id"),
      )) === "researcher",
  );

  await p.locator('.react-flow__node[data-id="builder"]').click();
  await settle(p, 200);
  const builderPanel = p.getByRole("dialog", {
    name: "Builder · Codex",
    exact: true,
  });
  check(
    "a click picks the member: its panel opens without being modal, and its card is selected",
    (await builderPanel.count()) === 1 &&
      (await builderPanel.getAttribute("aria-modal")) === null &&
      (await p
        .locator('.react-flow__node.selected[data-id="builder"]')
        .count()) === 1,
  );
  const place = await p.evaluate(() => {
    const panel = document
      .querySelector('[role="dialog"]:not([aria-modal])')
      ?.getBoundingClientRect();
    const flow = document.querySelector(".react-flow")?.getBoundingClientRect();
    return panel && flow
      ? {
          right: flow.right - panel.right,
          top: panel.top - flow.top,
          width: panel.width,
          height: panel.height,
          flowHeight: flow.height,
        }
      : null;
  });
  check(
    "the panel sits at the top right of the map, 320 px wide, and leaves most of the map open",
    place !== null &&
      Math.abs(place.right - 12) <= 2 &&
      Math.abs(place.top - 12) <= 2 &&
      Math.abs(place.width - 320) <= 2 &&
      place.height < place.flowHeight * 0.8,
    JSON.stringify(place),
  );
  check(
    "the panel shows the task, its done-when line, the latest activity and the diff",
    (await builderPanel.getByText("login form", { exact: true }).count()) ===
      1 &&
      (await builderPanel
        .getByText(
          "The login form submits through the new adapter and its tests pass.",
          { exact: true },
        )
        .count()) === 1 &&
      (await builderPanel.getByText(/^editing LoginForm\.tsx · /).count()) ===
        1 &&
      (await builderPanel.getByText("+120", { exact: true }).count()) === 1,
  );

  await builderPanel
    .getByRole("button", { name: "Reviewer", exact: true })
    .click();
  await settle(p, 200);
  check(
    "a relation picks the member it names",
    (await p
      .getByRole("dialog", { name: "Reviewer · Claude Code", exact: true })
      .count()) === 1,
  );

  await p
    .locator('.react-flow__node[data-id="builder"]')
    .click({ button: "right" });
  await settle(p, 200);
  check(
    "right-click on a card opens its actions, with Connect to… as a submenu",
    (await p
      .getByRole("menuitem", { name: "Open terminal", exact: true })
      .count()) === 1 &&
      (await p.getByRole("menuitem", { name: "Reassign role" }).count()) ===
        1 &&
      (await p
        .getByRole("menuitem", { name: "Connect to…", exact: true })
        .count()) === 1 &&
      (await p
        .getByRole("menuitem", { name: "Dismiss", exact: true })
        .count()) === 1,
  );
  await p.getByRole("menuitem", { name: "Open terminal", exact: true }).click();
  await settle(p, 400);
  const modal = p.locator('main > [role="dialog"]');
  // The sheet of the same member is open too, so the modal is found by its title.
  check(
    "Open terminal opens a modal dialog titled with the member",
    (await modal
      .getByRole("heading", { name: "Builder · Codex", exact: true })
      .count()) === 1,
  );
  const size = await p.evaluate(() => {
    const main = document.querySelector("main")?.getBoundingClientRect();
    const dialog = document
      .querySelector('main > [role="dialog"]')
      ?.getBoundingClientRect();
    return main && dialog
      ? {
          width: dialog.width / main.width,
          height: dialog.height / main.height,
        }
      : null;
  });
  check(
    "the dialog covers about 90% of the central area",
    size !== null &&
      Math.abs(size.width - 0.9) < 0.02 &&
      Math.abs(size.height - 0.9) < 0.02,
    JSON.stringify(size),
  );
  check(
    "the dialog says the output is simulated",
    (await p.getByText(/^Simulated output:/).count()) === 1,
  );
  await settle(p, 300);
  await p.evaluate(axeSource);
  const axeModal = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the card and the terminal dialog open",
    axeModal.length === 0,
    axeModal.join(" || "),
  );

  await p.keyboard.press("Escape");
  await settle(p, 300);
  check(
    "Escape closes the terminal dialog, and the focus goes back to the card",
    (await modal.count()) === 0 &&
      (await p.evaluate(() =>
        document.activeElement?.getAttribute("data-team-id"),
      )) === "builder",
  );

  await p.locator('[data-team-id="tester"]').focus();
  await p.keyboard.press("Enter");
  await settle(p, 300);
  check(
    "Enter on a focused card opens its terminal dialog",
    (await modal.count()) === 1,
  );
  await p.keyboard.press("Escape");
  await settle(p, 300);

  await p.locator('[data-team-id="lead"]').focus();
  await p.keyboard.press("Shift+F10");
  await settle(p, 200);
  check(
    "Shift+F10 on a focused card opens its actions",
    (await p
      .getByRole("menuitem", { name: "Open terminal", exact: true })
      .count()) === 1,
  );
  await p.keyboard.press("Escape");
  await settle(p, 200);

  // Connect to…: linking two members draws a new dashed line, and the menu shows it as checked.
  await p
    .locator('.react-flow__node[data-id="researcher"]')
    .click({ button: "right" });
  await settle(p, 200);
  await openSubmenu(p, "Connect to…");
  await p
    .getByRole("menuitemcheckbox", { name: "Tester · OMP", exact: true })
    .click();
  await settle(p, 300);
  check(
    "Connect to… draws a new pair line between the two members",
    (await teamEdges(p)) === 5,
    `${await teamEdges(p)} edges`,
  );
  await p
    .locator('.react-flow__node[data-id="researcher"]')
    .click({ button: "right" });
  await settle(p, 200);
  await openSubmenu(p, "Connect to…");
  check(
    "the submenu marks the linked member as checked",
    (await p
      .getByRole("menuitemcheckbox", { name: "Tester · OMP", exact: true })
      .getAttribute("aria-checked")) === "true",
  );
  await p.keyboard.press("Escape");
  await settle(p, 200);

  // The × on the picked member's panel removes the line again.
  await p.locator('.react-flow__node[data-id="researcher"]').click();
  await settle(p, 200);
  const researcherPanel = p.getByRole("dialog", {
    name: "Researcher · OpenCode",
    exact: true,
  });
  await researcherPanel
    .getByRole("button", { name: "Disconnect from Tester", exact: true })
    .click();
  await settle(p, 300);
  check(
    "the × on the panel removes the pair, and the panel stays open",
    (await teamEdges(p)) === 4 && (await researcherPanel.count()) === 1,
  );

  // Expanding Builder shows its tester; Alt+Up/Down then moves the card, and its tester with it.
  await p.getByRole("button", { name: "Expand Builder", exact: true }).click();
  await settle(p, 300);
  check(
    "expanding Builder shows its tester under it",
    (await teamNodes(p)) === 6,
  );
  const before = {
    builder: await cardAt(p, "builder"),
    test: await cardAt(p, "login-tests"),
  };
  await p.locator('[data-team-id="builder"]').focus();
  await p.keyboard.press("Alt+ArrowDown");
  await settle(p, 300);
  const down = {
    builder: await cardAt(p, "builder"),
    test: await cardAt(p, "login-tests"),
  };
  const step = down.builder.y - before.builder.y;
  check(
    "Alt+Down moves a card one row down, and the card under it moves too",
    step > 0 &&
      Math.abs(down.test.y - before.test.y - step) < 1 &&
      // Its own member stays right under it, wherever the row packing puts the pair.
      Math.abs(down.test.x - down.builder.x) < 1,
    JSON.stringify({ before, down }),
  );
  await p.keyboard.press("Alt+ArrowUp");
  await settle(p, 300);
  const back = await cardAt(p, "builder");
  check(
    "Alt+Up moves the card back to its row",
    Math.abs(back.y - before.builder.y) < 1,
    JSON.stringify({ back, before: before.builder }),
  );

  // A mouse drag: only the height follows the pointer, and the card lands on the row it is over.
  const zoom = await mapZoom(p);
  const reviewerBefore = await cardAt(p, "researcher");
  const card = await p.locator('[data-team-id="researcher"]').boundingBox();
  const cx = card.x + card.width / 2;
  const cy = card.y + card.height / 2;
  await p.mouse.move(cx, cy);
  await p.mouse.down();
  await p.mouse.move(cx + 10, cy + 40 * zoom, { steps: 4 });
  await p.mouse.move(cx, cy + 176 * zoom, { steps: 10 });
  await p.mouse.up();
  await settle(p, 300);
  const reviewerAfter = await cardAt(p, "researcher");
  check(
    "dragging a card down snaps it one row lower",
    Math.abs(reviewerAfter.y - reviewerBefore.y - 176) < 2,
    JSON.stringify({ reviewerBefore, reviewerAfter, zoom }),
  );
  // The cards it left on the row sit side by side again: 224 px wide and 32 apart, no gap where it was.
  const stayed = [];
  for (const id of ["builder", "reviewer", "tester"]) {
    stayed.push(await cardAt(p, id));
  }
  stayed.sort((a, b) => a.x - b.x);
  check(
    "the cards it left behind close up around the gap",
    stayed.every(
      (card, i) => i === 0 || Math.abs(card.x - stayed[i - 1].x - 256) < 2,
    ),
    JSON.stringify(stayed),
  );

  // A handle drag draws a pair too: any handle can join any handle.
  const builderHandle = await p
    .locator('[data-nodeid="builder"][data-handleid="right"]')
    .boundingBox();
  const testerHandle = await p
    .locator('[data-nodeid="tester"][data-handleid="left"]')
    .boundingBox();
  await p.locator('[data-team-id="builder"]').hover();
  await p.mouse.move(
    builderHandle.x + builderHandle.width / 2,
    builderHandle.y + builderHandle.height / 2,
  );
  await p.mouse.down();
  await p.mouse.move(
    testerHandle.x + testerHandle.width / 2,
    testerHandle.y + testerHandle.height / 2,
    { steps: 10 },
  );
  await p.mouse.up();
  await settle(p, 300);
  check(
    "dragging from a side handle to another card draws a pair",
    (await teamEdges(p)) === 6,
    `${await teamEdges(p)} edges`,
  );

  // The Delete key removes a picked line.
  await clickEdge(p, "builder|tester");
  await settle(p, 200);
  await p.keyboard.press("Delete");
  await settle(p, 300);
  check(
    "Delete removes the picked pair line",
    (await teamEdges(p)) === 5,
    `${await teamEdges(p)} edges`,
  );
  // A line of the tree can be picked too: taking it away leaves the member answering to no one, on the top row.
  await clickEdge(p, "lead-builder");
  await settle(p, 200);
  await p.keyboard.press("Delete");
  await settle(p, 300);
  const leadCard = await cardAt(p, "lead");
  const loose = await cardAt(p, "builder");
  check(
    "Delete removes a line of the tree and the member goes to the top row",
    (await teamEdges(p)) === 4 && Math.abs(loose.y - leadCard.y) < 2,
    JSON.stringify({ edges: await teamEdges(p), leadCard, loose }),
  );
  // From the orchestrator's bottom handle to the member's top handle, it answers to it again.
  const drag = async (fromId, fromHandle, toId, toHandle) => {
    await p.locator(`[data-team-id="${fromId}"]`).hover();
    const from = await p
      .locator(`[data-nodeid="${fromId}"][data-handleid="${fromHandle}"]`)
      .boundingBox();
    const to = await p
      .locator(`[data-nodeid="${toId}"][data-handleid="${toHandle}"]`)
      .boundingBox();
    await p.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await p.mouse.down();
    await p.mouse.move(to.x + to.width / 2, to.y + to.height / 2, {
      steps: 10,
    });
    await p.mouse.up();
    await settle(p, 400);
  };
  await drag("lead", "bottom", "builder", "top");
  const reattached = await cardAt(p, "builder");
  check(
    "dragging from a bottom handle to a top handle puts the member under the orchestrator",
    (await teamEdges(p)) === 5 && reattached.y > leadCard.y + 100,
    JSON.stringify({ edges: await teamEdges(p), reattached }),
  );
  check(
    "only an orchestrator has a bottom handle",
    (await p
      .locator('[data-nodeid="tester"][data-handleid="bottom"]')
      .count()) === 0 &&
      (await p
        .locator('[data-nodeid="lead"][data-handleid="bottom"]')
        .count()) === 1,
  );

  await p.locator('.react-flow__node[data-id="builder"]').click();
  await settle(p, 200);
  await p
    .getByRole("dialog", { name: "Builder · Codex", exact: true })
    .getByRole("button", { name: "Changes", exact: true })
    .click();
  await settle(p, 300);
  check(
    "the panel's Changes button opens the Changes tab of the right panel",
    (await p
      .getByRole("tab", { name: "Changes", exact: true, selected: true })
      .count()) === 1,
  );

  await settle(p, 300);
  await p.evaluate(axeSource);
  const axe = await p.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ["wcag2a", "wcag2aa"] });
    return r.violations.map(
      (v) => `${v.id} (${v.nodes.length}): ${v.nodes[0].html.slice(0, 120)}`,
    );
  });
  check(
    "axe: no WCAG A/AA violations with the map and the panel open",
    axe.length === 0,
    axe.join(" || "),
  );

  const projects = p.getByRole("group", {
    name: "Which projects",
    exact: true,
  });
  await projects.getByRole("button", { name: "Active", exact: true }).click();
  await settle(p, 300);
  check(
    "Active lists only the projects with a running terminal",
    (await p.getByText("api-server", { exact: true }).count()) === 0 &&
      (await p.getByText("lynx-ide", { exact: true }).count()) >= 1,
  );
  await projects.getByRole("button", { name: "All", exact: true }).click();
  await settle(p, 300);
  check(
    "All lists every project again",
    (await p.getByText("api-server", { exact: true }).count()) === 1,
  );

  await p.keyboard.press("Control+Shift+B");
  await settle(p, 300);
  check(
    "with the sidebar hidden, the team bar keeps a Terminal button",
    (await p.getByRole("button", { name: "Terminal", exact: true }).count()) ===
      1,
  );
  await p.getByRole("button", { name: "Terminal", exact: true }).click();
  await settle(p, 300);
  check(
    "that button brings back the terminal tabs",
    (await p.getByRole("tablist", { name: "Terminals" }).count()) === 1,
  );
});

// A narrow window keeps the map: the picked member's panel still opens over it.
await session("?mock=full", 1000, 860, async (p) => {
  await p.getByRole("button", { name: /^feat\/auth/ }).click();
  await settle(p, 300);
  await p.getByRole("button", { name: "Team", exact: true }).click();
  await p.locator(".react-flow__node").first().waitFor({ timeout: 15000 });
  await settle(p, 400);
  check(
    "a narrow window shows the team as the map, with the question in the panel and no Reply",
    (await p.locator(".react-flow__node").count()) === 5 &&
      (await p
        .getByRole("dialog", { name: "Researcher · OpenCode", exact: true })
        .count()) === 1 &&
      (await p.getByRole("button", { name: "Reply", exact: true }).count()) ===
        0,
  );
  check(
    "no horizontal scroll with the team in a narrow window",
    (await overflow(p)) <= 0,
  );
});

await runQueuedSessions();
console.log(results.join("\n"));
await browser.close();
vite.kill();
process.exit(results.some((line) => line.startsWith("FAIL")) ? 1 : 0);
