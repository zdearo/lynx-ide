# Lynx

**Lynx** (`lynx-ide`) é um ADE (Agentic Development Environment) desktop: roda vários agentes de código em paralelo (Claude Code, Codex, OpenCode, OMP), cada um isolado no seu próprio `git worktree`, com terminais, estado, diff, PR e browser reunidos num lugar só.

Versão atual: **0.4.0**.

## O que faz

- **Worktrees como unidade de trabalho**: cada tarefa ganha um worktree com terminais, abas, diff e browser próprios. Projetos organizados em pastas, inclusive pastas vinculadas a um diretório do disco.
- **Agentes com status**: trabalhando, precisa de você, concluído ou ocioso, detectados por hooks do agente e pela saída do terminal. Notificações do sistema e retomada de sessões encerradas.
- **Editor e Git**: explorador de arquivos, editor (CodeMirror), diff unificado/lado a lado, stage, commit (com mensagem gerada pelo CLI do agente), push, pull e amend.
- **Pull requests** via `gh`: criar, ver checks e logs, discussão, merge e limpeza do worktree depois do merge.
- **Browser embutido** (Chromium/CEF) que os agentes podem controlar por ferramentas `browser_*`.
- **Orquestração**: um agente ou bot coordena workers em outros worktrees.
- **Bots**: assistentes de chat com memória, threads, canais e acesso controlado a projetos.
- **Limites dos planos** de Claude e Codex no rodapé.
- **Atualização automática**.

## Download

Instaladores são publicados pelo CI a cada release (tag `v*`) em `downloads.zdearo.dev`:

| Sistema | Formatos |
|---|---|
| Linux x64 | `.AppImage`, `.deb`, `.rpm` |
| Windows x64 | `.exe` (NSIS), `.msi` |
| macOS | ainda não |

O app se atualiza sozinho (Configurações → Updates). No Linux, só o AppImage se autoatualiza; `.deb` e `.rpm` ficam com o gerenciador de pacotes.

O motor do browser (~140 MB) não vem no instalador: é baixado de dentro do app na primeira vez (Configurações → Browser).

### Requisitos em tempo de uso

- `git` 2.25+
- Pelo menos um agente no `PATH`: `claude`, `codex`, `opencode` ou `omp`
- `gh` logado, para as funções de pull request (opcional)

## Build a partir do código

### Pré-requisitos

- [Bun](https://bun.sh)
- Rust estável (fixado em `rust-toolchain.toml`, com `rustfmt` e `clippy`)
- Dependências do Tauri v2 para o sistema: <https://v2.tauri.app/start/prerequisites/>
  - Linux: GTK 3 e WebKitGTK 4.1
  - Windows: WebView2 e Build Tools do MSVC

### Rodar em desenvolvimento

```bash
bun install
bun run tauri dev
```

Build de debug usa dados separados (`~/.lynx-dev/<branch>`), nunca os do app instalado.

Para ver só a interface no navegador, com backend simulado: `bun run dev` e abra a URL com `?mock=full` (ou `?mock=empty`).

### Gerar o pacote

```bash
bun run tauri build              # instaladores em src-tauri/target/release/bundle/
bun run tauri build --no-bundle  # só o binário
```

Com `createUpdaterArtifacts` ligado, o build exige `TAURI_SIGNING_PRIVATE_KEY` no ambiente. O instalador do Windows é gerado no CI (`.github/workflows/build.yml`).

### Motor do browser (desenvolvimento)

```bash
bun run browser:engine            # monta engine/current para desenvolvimento
bun run browser:engine -- --bundle  # gera o pacote distribuível
```

## Comandos

| Comando | O que faz |
|---|---|
| `bun run check` | typecheck, ESLint, Prettier e Vitest |
| `bun run rust:check` | `cargo fmt --check` e `clippy -D warnings` |
| `bun run rust:test` | testes do backend (inclui sincronia dos bindings) |
| `bun run bindings` | regenera `src/bindings.ts` a partir do Rust |
| `bun run e2e` | testes de interface no Chrome headless (inclui axe WCAG) |
| `bun run bench <rótulo>` | benchmarks de backend e frontend |
| `bun run format` | formata com Prettier |

## Stack

Tauri 2 · Rust (edition 2024) · React 19 · TypeScript · Vite · Tailwind 4 · shadcn/ui · xterm.js · CodeMirror 6 · SQLite · CEF.

## Estrutura

```
src/             frontend React (UI, stores, textos em ui-copy/)
src-tauri/       backend Rust (processos, PTYs, Git, persistência, IPC)
  crates/        browser-protocol, browser-host, pty-daemon
services/        update-worker (Cloudflare Worker de atualizações)
e2e/             testes de interface e benchmarks
scripts/         build e publicação
```

## Configuração e dados

- Preferências: `~/.lynx/config.json` (editável à mão; `LYNX_HOME` troca a pasta).
- Estado do app: `app.db` (SQLite) na pasta de dados do sistema.
- Logs: `~/.lynx/logs/lynx.log`.

## Documentação

- [AGENTS.md](./AGENTS.md): arquitetura e convenções em detalhe.
- [PRODUCT.md](./PRODUCT.md): produto e público.
- [TODO.md](./TODO.md): fases e tarefas.
- [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md): licenças de terceiros.
