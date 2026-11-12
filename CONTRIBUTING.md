# Contributing to Lynx

Thanks for your interest. This file covers how to report problems, how to run the code, and how
contributions are licensed.

## Where things happen

- **Bugs and feature requests:** open an issue.
- **Large changes:** open an issue first, so we agree on the direction before you write code.
- **Security problems:** never a public issue. See [SECURITY.md](SECURITY.md).

## Run it for development

You need [Bun](https://bun.sh) and the Rust stable toolchain (pinned in `rust-toolchain.toml`).

```sh
bun install
bun run tauri dev      # the desktop app with hot reload
```

Before opening a pull request, run the checks that apply to your change:

| Change         | Command                                                                    |
| -------------- | -------------------------------------------------------------------------- |
| Frontend       | `bun run check`                                                            |
| Backend (Rust) | `bun run rust:check` and `bun run rust:test`                               |
| IPC types      | `bun run bindings` (`src/bindings.ts` is generated, never edit it by hand) |
| UI             | `bun run e2e` (headless Chrome against the mock backend)                   |

[AGENTS.md](AGENTS.md) describes the architecture and the conventions in detail.

## Pull requests

- Keep them focused, and add or update tests where behavior changes.
- Commit messages and PR titles follow [Conventional Commits](https://www.conventionalcommits.org),
  with a scope where one fits: `feat(terminal): ...`, `fix(changes): ...`.
- Code, identifiers, logs and commit messages are in English.

## Licensing of your contribution

- Lynx is licensed under the [AGPL-3.0-only](LICENSE).
- Inbound = outbound: your contribution is licensed under the license of the code it goes into. You
  keep the copyright in what you write. [GitHub's Terms of Service](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service)
  (section D.6) already license contributions under the repository's license.
- There is no contributor license agreement to sign and no DCO sign-off.
- Submit only your own code, or code under MIT, BSD, ISC, Apache-2.0 or MPL-2.0 (whole files, with
  their notices). Never GPL, AGPL, unlicensed or proprietary code.
- AI-assisted code is fine. You answer for its origin.
