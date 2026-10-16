# AGENTS.md

Contexto do projeto para agentes de IA (Claude Code, Codex e similares) que trabalham aqui. Leia antes de mexer no código. As tarefas e a ordem de execução estão em [TODO.md](./TODO.md).

## O projeto

**agent-ade** é um ADE (Agentic Development Environment) desktop. Ele roda vários agentes de código em paralelo (Claude Code, Codex, OpenCode e outros CLIs), cada um isolado no seu próprio `git worktree`, com terminais, estado e revisão reunidos num lugar só.

O foco atual é o **núcleo**: worktrees, terminais de agentes e estado básico. Revisão de diff, integrações (GitHub, Linear, Jira), orquestração e CLI própria entram depois, nas fases do TODO.

## Referência: Orca

O Orca (`stablyai/orca`, licença MIT) é a principal referência de produto e de arquitetura. Clone local:

- `/home/user/Projects/Coding/git-clone/orca`
- Documentação do modelo de produto: `docs/site/content/docs/model/` nesse clone.

Use o Orca como referência de comportamento e de decisões de arquitetura. Não copie código dele: a implementação é escrita aqui.

## Stack

- **Desktop:** Tauri v2
- **Backend:** Rust (comandos Tauri, processos, PTYs, chamadas ao Git)
- **Frontend:** React + TypeScript + Vite
- **UI:** Tailwind CSS + shadcn/ui (primitivos em `src/components/ui/`)
- **Terminal:** xterm.js
- **Plataformas:** Linux (ambiente atual), macOS e Windows

Versões atuais: Tauri 2.12, tauri-specta 2.0.0-rc.25, React 19, Vite 8, TypeScript 6, Tailwind 4, shadcn (preset Nova, base Radix, ícones Lucide), Rust stable 1.99 (edition 2024).

## Estrutura

- `src/`: frontend React. Primitivos shadcn em `src/components/ui/`, utilitários de UI em `src/lib/`. O alias `@/` aponta para `src/`.
- `src-tauri/`: backend Rust. `src/lib.rs` monta o app; `src/ipc.rs` é o registro único de comandos e eventos; `src/main.rs` só chama `agent_ade_lib::run()`.
- `src/bindings.ts`: contrato IPC gerado pelo Rust. Fica versionado, mas é ignorado pelo ESLint e pelo Prettier.
- `src-tauri/capabilities/`: permissões do Tauri por janela. Adicione só o que o código usa.
- `rust-toolchain.toml`: fixa a toolchain estável com rustfmt e clippy.
- Tema escuro por padrão (`class="dark"` em `index.html`). Tokens de cor em `src/index.css`.

## Princípios de arquitetura

1. **O worktree é a unidade de tudo.** Terminais, abas, layout e diff pertencem a um worktree.
2. **O backend Rust é dono dos processos e do estado.** PTYs, worktrees e persistência ficam no Rust. O React renderiza e envia comandos pelo IPC do Tauri. Assim a mesma lógica pode servir depois a uma CLI e a um companion mobile.
3. **Agnóstico ao agente.** O estado (trabalhando, precisa de você, ocioso) é detectado pelo que o agente imprime no terminal e por hooks. Nunca depender de API proprietária de um provedor.
4. **Git pelo binário do usuário.** Operações de worktree e branch chamam o `git` instalado, via subprocesso, como o Orca faz. Assim valem a config, os hooks e as credenciais do próprio usuário. Baseline: Git 2.25.
5. **Multiplataforma desde o início.** Monte caminhos com `PathBuf` e `Path::join`, nunca com `/` ou `\` fixos. Atalhos de teclado usam `Cmd` no macOS e `Ctrl` nos demais.
6. **Escopo enxuto.** Não adiante funções de fases posteriores dentro do núcleo.

## Convenções

- **Idioma:** documentação e comunicação com o dono do projeto em português (pt-BR). Código, identificadores, nomes de arquivo, logs e mensagens de commit em inglês.
- **Nomes:** não use `helpers`, `utils`, `common`, `misc` nem `shared-stuff`. Nomeie pelo conceito que o arquivo contém, por exemplo `worktree_manager.rs` ou `terminal_session.rs`. Se o nome genérico parecer necessário, o arquivo provavelmente tem responsabilidades demais e deve ser dividido.
- **Comentários:** curtos, uma linha quando possível, só para o porquê não óbvio.
- **Rust:**
  - `cargo fmt` e `cargo clippy -- -D warnings` limpos.
  - Erros tipados com `thiserror`.
  - Sem `unwrap()` nem `expect()` fora de testes e da inicialização.
  - Comandos Tauri são finos: validam a entrada e delegam ao módulo de domínio.
- **Frontend:**
  - Antes de criar um componente, verifique se o shadcn já cobre o caso.
  - Cores, raios e espaçamentos vêm dos tokens do tema (variáveis CSS), não de valores fixos.
- **Contrato IPC:** os tipos de comandos e eventos são definidos só no Rust (`tauri-specta`) e o TypeScript é gerado em `src/bindings.ts`. Nunca edite esse arquivo à mão e não chame `invoke("...")` direto: use `commands.*` de `@/bindings`. Para criar um comando: derive `serde::Serialize` e `specta::Type` nos tipos, anote a função com `#[tauri::command]` e `#[specta::specta]`, registre em `src-tauri/src/ipc.rs` (`collect_commands!`) e rode `pnpm bindings`. O teste `committed_bindings_match_rust_contract` falha se o arquivo ficar desatualizado. As versões de `specta` e `tauri-specta` estão fixadas com `=` porque ainda são release candidates: atualize-as juntas e deliberadamente.
- **Testes:** a lógica de Git é testada com repositórios temporários criados dentro do próprio teste (crate `tempfile`).
- **Commits:** só quando o dono do projeto pedir.

## Estado atual

- Fase 0 concluída (ver TODO.md). O app abre uma janela com sidebar vazia e um comando IPC de exemplo (`app_info`). Ainda não há lógica de worktree nem de terminal: a Fase 1 começa daqui.
- Ambiente de desenvolvimento: Arch Linux (Hyprland), Node 24 e pnpm. O Rust padrão da máquina é nightly, mas o projeto usa a estável via `rust-toolchain.toml`. As bibliotecas GTK e WebKit 4.1 já estão instaladas.

## Comandos

Gerenciador de pacotes: pnpm.

- `pnpm tauri dev`: sobe o Vite e o app desktop com hot reload.
- `pnpm tauri build`: gera o pacote de release. `pnpm tauri build --no-bundle` só compila o binário.
- `pnpm check`: typecheck, ESLint, Prettier (verificação) e Vitest do frontend. Rode antes de concluir uma tarefa no frontend.
- `pnpm rust:check`: `cargo fmt --check` e `cargo clippy -D warnings`.
- `pnpm rust:test`: testes do backend (inclui a checagem de sincronia dos bindings).
- `pnpm bindings`: regenera `src/bindings.ts` a partir do Rust.
- `pnpm format`: formata o projeto com Prettier.
- Adicionar componente shadcn: `pnpm dlx shadcn@latest add <componente>`.

`pnpm tauri dev` abre uma janela na tela do usuário. Evite rodá-lo sem necessidade e nunca tire capturas de tela do monitor para validar a UI: a captura pode pegar outras janelas do usuário.

## Fora de escopo por enquanto

Orquestração multi-agente, CLI própria, automações agendadas, execução remota por SSH, companion mobile, browser embutido e pastas que não são repositórios git.
