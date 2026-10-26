/**
 * Runs the backend and frontend measurements and keeps the result in `bench/<label>.json`.
 *   bun run bench <label>            measure now
 *   bun run bench compare <a> <b>    show two saved results side by side
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

const [command, ...rest] = process.argv.slice(2);

function run(file, args) {
  const result = spawnSync(file, args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    process.stderr.write(result.stdout + result.stderr);
    process.exit(result.status ?? 1);
  }
  return result.stdout;
}

if (command === "compare") {
  const [a, b] = rest.map((label) =>
    JSON.parse(readFileSync(`bench/${label}.json`, "utf8")),
  );
  const rows = [];
  for (const scenario of Object.keys({ ...a.results, ...b.results })) {
    for (const key of Object.keys({
      ...a.results[scenario],
      ...b.results[scenario],
    })) {
      const before = a.results[scenario]?.[key];
      const after = b.results[scenario]?.[key];
      const change =
        before && after !== undefined
          ? `${(((after - before) / before) * 100).toFixed(0)}%`
          : "";
      rows.push([`${scenario}.${key}`, before ?? "-", after ?? "-", change]);
    }
  }
  const width = Math.max(...rows.map((row) => row[0].length));
  console.log(
    `${"metric".padEnd(width)}  ${rest[0].padStart(12)}  ${rest[1].padStart(12)}  change`,
  );
  for (const [name, before, after, change] of rows)
    console.log(
      `${name.padEnd(width)}  ${String(before).padStart(12)}  ${String(after).padStart(12)}  ${change}`,
    );
  process.exit(0);
}

const label = command;
if (!label) {
  console.error("usage: bench <label> | bench compare <a> <b>");
  process.exit(2);
}

const results = {};

const median = (values) =>
  [...values].sort((x, y) => x - y)[values.length >> 1];
const samples = {};
for (let i = 0; i < Number(process.env.BENCH_BACKEND_RUNS ?? 3); i++) {
  const backend = run("cargo", [
    "test",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--profile",
    "perf",
    "-p",
    "lynx-ide",
    "perf_tests::bench",
    "--",
    "--ignored",
    "--nocapture",
    "--test-threads=1",
  ]);
  for (const line of backend.split("\n")) {
    const match = line.match(/BENCH (\w+) (.*)$/);
    if (!match) continue;
    for (const pair of match[2].split(" ")) {
      const [key, value] = pair.split("=");
      ((samples[match[1]] ??= {})[key] ??= []).push(Number(value));
    }
  }
}
for (const [name, keys] of Object.entries(samples))
  results[name] = Object.fromEntries(
    Object.entries(keys).map(([key, values]) => [key, median(values)]),
  );

const frontend = run("node", ["e2e/perf-bench.mjs"]);
const json = frontend
  .split("\n")
  .find((line) => line.startsWith("BENCH_JSON "));
if (!json) {
  console.error(frontend);
  process.exit(1);
}
for (const [scenario, values] of Object.entries(
  JSON.parse(json.slice("BENCH_JSON ".length)),
))
  results[`ui_${scenario}`] = values;

mkdirSync("bench", { recursive: true });
writeFileSync(
  `bench/${label}.json`,
  JSON.stringify(
    {
      label,
      date: new Date().toISOString(),
      commit: run("git", ["rev-parse", "--short", "HEAD"]).trim(),
      results,
    },
    null,
    2,
  ) + "\n",
);
console.log(`saved bench/${label}.json`);
console.log(JSON.stringify(results, null, 2));
