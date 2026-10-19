/** The same rules the backend checks, so the form can refuse a name before asking GitHub. */

/** Letters, digits and hyphens; no leading or trailing hyphen; up to 39 characters. */
export function isValidOwner(owner: string): boolean {
  return /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?$/.test(owner);
}

/** Letters, digits, hyphens, underscores and dots; not `.` or `..`; up to 100 characters. */
export function isValidRepoName(name: string): boolean {
  return name !== "." && name !== ".." && /^[A-Za-z0-9._-]{1,100}$/.test(name);
}
