/**
 * Interface measurements in headless Chrome against the in-memory backend (`?mock=bench`):
 * startup, a burst of terminal output across many mounted terminals, and a folder of 5000 files.
 * Run with `bun run bench:frontend`. Prints `BENCH_JSON {...}`; `scripts/bench.mjs` collects it.
 * Set CHROME_PATH when Chrome is not at a standard location, and BENCH_RUNS for more samples.
 */
import { spawn } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { chromium } from "playwright-core";

const PORT = Number(process.env.BENCH_PORT ?? 1432);
const BASE = `http://localhost:${PORT}/`;
const RUNS = Number(process.env.BENCH_RUNS ?? 3);
const require = createRequire(import.meta.url);

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

const viteBin = join(
  dirname(require.resolve("vite/package.json")),
  "bin",
  "vite.js",
);
const vite = spawn(
  process.execPath,
  [viteBin, "--port", String(PORT), "--strictPort"],
  { stdio: "ignore" },
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

// A failed run must not leave the server behind: the next run would talk to it.
process.on("exit", () => vite.kill());

const browser = await chromium.launch({
  executablePath: chromePath,
  headless: true,
  args: ["--no-sandbox"],
});

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
};

/** Resident memory of the browser and everything it started, in megabytes (Linux only). */
function browserTreeRssMb() {
  const processes = new Map();
  for (const pid of readdirSync("/proc").filter((name) => /^\d+$/.test(name))) {
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      const rest = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
      const cmdline = readFileSync(`/proc/${pid}/cmdline`, "utf8");
      const status = readFileSync(`/proc/${pid}/status`, "utf8");
      const rss = Number(/VmRSS:\s+(\d+)/.exec(status)?.[1] ?? 0) / 1024;
      processes.set(Number(pid), { parent: Number(rest[1]), cmdline, rss });
    } catch {
      // The process ended while it was read.
    }
  }
  const root = [...processes].find(
    ([, p]) =>
      p.cmdline.includes("playwright_chromiumdev_profile") &&
      !p.cmdline.includes("--type="),
  )?.[0];
  if (!root) return NaN;
  const inTree = new Set([root]);
  for (let grew = true; grew;) {
    grew = false;
    for (const [pid, p] of processes)
      if (!inTree.has(pid) && inTree.has(p.parent)) {
        inTree.add(pid);
        grew = true;
      }
  }
  return [...inTree].reduce((sum, pid) => sum + processes.get(pid).rss, 0);
}

async function metrics(client) {
  const { metrics } = await client.send("Performance.getMetrics");
  return Object.fromEntries(metrics.map((m) => [m.name, m.value]));
}

// Warm the dev server so the first run does not pay for dependency optimisation.
async function open() {
  const context = await browser.newContext({
    viewport: { width: 1400, height: 800 },
  });
  const page = await context.newPage();
  // The DOM renderer: headless Chrome has no GPU, and the cost under test is the page's own.
  await page.addInitScript(() => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (type, ...rest) {
      return /^webgl/.test(type) ? null : getContext.call(this, type, ...rest);
    };
    window.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries())
        window.__longTasks.push(entry.duration);
    }).observe({ entryTypes: ["longtask"] });
  });
  const client = await context.newCDPSession(page);
  await client.send("Performance.enable");
  return { context, page, client };
}

const runs = { startup: [], flood: [], files: [], memory: [], memoryLow: [] };

for (let run = -1; run < RUNS; run++) {
  const warmup = run < 0;

  // Startup: from navigation until all 38 extra repos have their worktrees in the sidebar.
  {
    const { context, page, client } = await open();
    const started = Date.now();
    await page.goto(BASE + "?mock=bench");
    // Done when the last repo has its worktrees listed, not only its header.
    await page
      .getByRole("button", { name: /^task\/3/ })
      .nth(37)
      .waitFor();
    const elapsed = Date.now() - started;
    const m = await metrics(client);
    if (!warmup)
      runs.startup.push({
        ms: elapsed,
        scriptMs: m.ScriptDuration * 1000,
        taskMs: m.TaskDuration * 1000,
        domNodes: m.Nodes,
        heapMb: m.JSHeapUsedSize / 1e6,
      });
    await context.close();
  }

  // Output across many mounted terminals: one visible, two hidden.
  {
    const { context, page, client } = await open();
    await page.goto(BASE + "?mock=bench", { waitUntil: "networkidle" });
    await page.getByRole("button", { name: /^feat\/auth/ }).click();
    await page.waitForTimeout(800);
    const ids = await page.evaluate(() => window.__lynxMock.sessionIds());
    const before = await metrics(client);
    await page.evaluate(() => (window.__longTasks.length = 0));
    const started = Date.now();
    await page.evaluate(
      ([a, b, c]) =>
        Promise.all([
          window.__lynxMock.flood(a, 1500, 4096),
          window.__lynxMock.flood(b, 1500, 4096),
          window.__lynxMock.flood(c, 1500, 4096),
        ]),
      ids.slice(0, 3),
    );
    // Give the page time to finish what it queued.
    await page.waitForFunction(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => setTimeout(() => resolve(true), 300)),
        ),
    );
    const elapsed = Date.now() - started;
    const after = await metrics(client);
    const longTasks = await page.evaluate(() => window.__longTasks);
    if (!warmup)
      runs.flood.push({
        ms: elapsed,
        scriptMs: (after.ScriptDuration - before.ScriptDuration) * 1000,
        taskMs: (after.TaskDuration - before.TaskDuration) * 1000,
        longTasks: longTasks.length,
        worstTaskMs: Math.max(0, ...longTasks),
        heapMb: after.JSHeapUsedSize / 1e6,
      });
    await context.close();
  }

  // A folder with 5000 files, opened in the explorer.
  {
    const { context, page, client } = await open();
    await page.goto(BASE + "?mock=bench", { waitUntil: "networkidle" });
    await page.getByRole("button", { name: /^feat\/auth/ }).click();
    await page.waitForTimeout(500);
    await page.keyboard.press("Control+Shift+E");
    const item = (name) => page.getByRole("treeitem", { name });
    await item(/^big(,|$)/).waitFor();
    await page.evaluate(() => (window.__longTasks.length = 0));
    const before = await metrics(client);
    const started = Date.now();
    await item(/^big(,|$)/).click({ timeout: 180_000 });
    // Counting in the page: a role locator over thousands of rows would be slower than the render.
    await page.waitForFunction(
      () =>
        [...document.querySelectorAll('[role="treeitem"]')].some((row) =>
          row.textContent?.includes("module-0000.ts"),
        ),
      null,
      { polling: 50, timeout: 180_000 },
    );
    const elapsed = Date.now() - started;
    const after = await metrics(client);
    const rows = await page.locator('[role="treeitem"]').count();
    // Scrolling to the last file: free when every row exists, a redraw when only a window does.
    const scrollStarted = Date.now();
    await page.evaluate(() => {
      const viewport = document
        .querySelector('[aria-label="Files in the selected worktree"]')
        ?.closest('[data-slot="scroll-area-viewport"]');
      if (viewport) viewport.scrollTop = viewport.scrollHeight;
    });
    await page.waitForFunction(
      () =>
        [...document.querySelectorAll('[role="treeitem"]')].some((row) =>
          row.textContent?.includes("module-4999.ts"),
        ),
      null,
      { polling: 50, timeout: 180_000 },
    );
    const scrollMs = Date.now() - scrollStarted;
    const longTasks = await page.evaluate(() => window.__longTasks);
    if (!warmup)
      runs.files.push({
        ms: elapsed,
        taskMs: (after.TaskDuration - before.TaskDuration) * 1000,
        rows,
        scrollToEndMs: scrollMs,
        domNodes: after.Nodes,
        worstTaskMs: Math.max(0, ...longTasks),
      });
    await context.close();
  }

  // Sixteen terminals, each printed past its scrollback: what the page keeps for them.
  for (const [name, scrollback] of [
    ["memory", null],
    ["memoryLow", 1000],
  ]) {
    const { context, page, client } = await open();
    await page.goto(BASE + "?mock=bench", { waitUntil: "networkidle" });
    await page.getByRole("button", { name: /^feat\/auth/ }).click();
    await page.waitForTimeout(800);
    if (scrollback)
      await page.evaluate(
        (value) => window.__lynxMock.editConfigFile({ scrollback: value }),
        scrollback,
      );
    await page.waitForTimeout(500);
    const ids = await page.evaluate(() => window.__lynxMock.sessionIds());
    const before = browserTreeRssMb();
    const heapBefore = (await metrics(client)).JSHeapUsedSize / 1e6;
    for (const id of ids)
      await page.evaluate((id) => window.__lynxMock.flood(id, 200, 4096), id);
    await page.waitForTimeout(2000);
    const after = browserTreeRssMb();
    const heapAfter = (await metrics(client)).JSHeapUsedSize / 1e6;
    if (!warmup)
      runs[name].push({
        terminals: ids.length,
        rssBeforeMb: before,
        rssAfterMb: after,
        rssPerTerminalMb: (after - before) / ids.length,
        jsHeapPerTerminalMb: (heapAfter - heapBefore) / ids.length,
      });
    await context.close();
  }
}

await browser.close();
vite.kill();

const summary = {};
for (const [scenario, samples] of Object.entries(runs)) {
  summary[scenario] = Object.fromEntries(
    Object.keys(samples[0]).map((key) => [
      key,
      Number(median(samples.map((s) => s[key])).toFixed(2)),
    ]),
  );
}
console.log("BENCH_JSON " + JSON.stringify(summary));
