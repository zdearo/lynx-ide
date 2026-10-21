// Semver 2.0.0. Build metadata is accepted but ignored when comparing.
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

export interface Version {
  core: [number, number, number];
  prerelease: string[];
}

export function parseVersion(text: string): Version | null {
  const match = SEMVER.exec(text);
  if (match === null) return null;
  return {
    core: [Number(match[1]), Number(match[2]), Number(match[3])],
    prerelease: match[4] === undefined ? [] : match[4].split("."),
  };
}

// Negative when a < b, positive when a > b, zero when they are equal.
export function compareVersions(a: Version, b: Version): number {
  for (let i = 0; i < 3; i++) {
    const diff = a.core[i] - b.core[i];
    if (diff !== 0) return diff;
  }

  // A release ranks above every prerelease of the same core version.
  if (a.prerelease.length === 0 || b.prerelease.length === 0) {
    return b.prerelease.length - a.prerelease.length;
  }

  for (let i = 0; ; i++) {
    const x: string | undefined = a.prerelease[i];
    const y: string | undefined = b.prerelease[i];
    if (x === undefined || y === undefined) {
      // When the shared part is equal, the shorter list ranks lower.
      return (x === undefined ? 0 : 1) - (y === undefined ? 0 : 1);
    }
    const diff = comparePrereleaseIdentifier(x, y);
    if (diff !== 0) return diff;
  }
}

function comparePrereleaseIdentifier(x: string, y: string): number {
  const xNumeric = /^\d+$/.test(x);
  const yNumeric = /^\d+$/.test(y);
  if (xNumeric && yNumeric) return Math.sign(Number(x) - Number(y));
  // Numeric identifiers rank below alphanumeric ones.
  if (xNumeric !== yNumeric) return xNumeric ? -1 : 1;
  if (x === y) return 0;
  return x < y ? -1 : 1;
}
