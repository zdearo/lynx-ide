# TODO — agent-ade

Ordem de execução: de cima para baixo. Cada fase depende da anterior. Contexto, princípios e convenções estão em [AGENTS.md](./AGENTS.md).

Legenda: `[ ]` pendente · `[x]` feito · **(?)** decisão em aberto (ver seção final).

---

## Fase 0 — Setup do projeto

- [x] Inicializar o repositório git e criar o `.gitignore` (Rust, Node, `dist/`, arquivos de ambiente)
- [x] Criar o app Tauri v2 com React, TypeScript e Vite
- [x] Fixar a toolchain Rust estável com `rust-toolchain.toml` (hoje o padrão é nightly)
- [x] Configurar Tailwind CSS e inicializar o shadcn/ui
- [x] Configurar lint e formatação: `cargo fmt`, `cargo clippy -- -D warnings`, ESLint e Prettier
- [x] Definir os scripts de `dev`, `build`, `test` e `typecheck` e registrá-los em `AGENTS.md`
- [x] Adotar `tauri-specta` para o contrato IPC (bindings gerados em `src/bindings.ts`, com teste de sincronia)
- [x] Confirmar que `tauri dev` abre a janela e que o build de release compila (`tauri build --no-bundle`)
- [ ] Validar o empacotamento (`tauri build` completo: deb, rpm, AppImage) no Linux

---

## Fase 1 — Núcleo

### 1.1 Domínio e persistência (Rust)

- [x] Modelo `Repo` (id, nome, caminho) e `WorktreeInfo` (caminho, branch, base ref, principal, bloqueado, órfão)
- [x] Validar que o caminho é um repositório git antes de registrá-lo (resolve para o worktree principal e recusa repositórios bare)
- [x] Persistir repos e a base ref de cada worktree em SQLite (`app.db` no diretório de dados do app), com migrações versionadas por `user_version`. A lista de worktrees vem sempre do Git, que é a fonte da verdade
- [x] Restaurar a lista de repos e worktrees ao abrir o app (backend; a interface vem na 1.5)

### 1.2 Worktrees (Git)

- [x] Listar worktrees com `git worktree list --porcelain`
- [x] Criar worktree com branch nova a partir de uma base (branch local, SHA ou branch remota), sem rastrear a branch remota
- [ ] Criar em segundo plano, com status visível (`fetch` e `worktree add`). O comando já roda fora da thread da UI; falta a linha de progresso na sidebar (1.5)
- [x] Colocar os worktrees fora da pasta do repo, em `<repo>-worktrees/<nome>` **(?)**
- [x] Remover worktree e branch; recusa alterações não commitadas, a menos que seja forçado (a confirmação na interface vem na 1.5)
- [x] Preservar a branch quando tiver commits não mergeados (`git branch -d` e motivo devolvido ao frontend)
- [x] Testes com repositórios temporários: criar, listar, remover, preservar branch, nomes inválidos, duplicados e worktree apagado por fora

### 1.3 Terminais (PTY)

- [x] Criar sessão de PTY com `portable-pty`, no diretório do worktree (shell de login do usuário; o comando é digitado no shell, então a aba continua viva quando o agente sai)
- [x] Comandos IPC: criar, listar, snapshot, enviar input, redimensionar e fechar
- [x] Enviar ao frontend os eventos de saída e de término (tipados, com scrollback de 1 MiB no Rust para reanexar a view sem perder nem duplicar saída)
- [x] Suportar várias sessões por worktree
- [x] Renderizar no frontend com xterm.js e o addon de fit (`TerminalView`; ainda não montado na janela, isso vem na 1.5)
- [ ] Escolher o comando ao criar a sessão: shell, `claude`, `codex`, `opencode` ou comando livre. O backend já aceita qualquer comando; falta o seletor na interface (1.5)
- [x] Encerrar todos os PTYs ao fechar o app, sem deixar processo órfão (SIGHUP no grupo em primeiro plano, SIGKILL após 3 s para quem ignora)

### 1.4 Estado dos agentes (básico)

- [ ] Marcar a sessão como "trabalhando" ou "ocioso" pela atividade de saída
- [ ] Ler o título OSC do terminal quando o agente o envia
- [ ] Guardar o estado num único store no Rust e exibi-lo na sidebar, por worktree
- [ ] Indicar quando uma sessão termina (trabalhando → ocioso)

### 1.5 Interface base

- [ ] Layout com sidebar (repos → worktrees) e área principal com abas de terminal
- [ ] Componentes shadcn: Button, Dialog, Input, Tabs, ScrollArea, Badge, Sonner, DropdownMenu e Tooltip
- [ ] Diálogo "Novo worktree" (nome e base)
- [ ] Atalhos básicos: nova sessão, aba anterior e próxima, trocar de worktree
- [ ] Tema claro e escuro

### Critério de pronto da Fase 1

Abrir um repo, criar dois worktrees, rodar um agente em cada um e ver os dois estados na sidebar. Fechar e reabrir o app sem perder a lista de repos e worktrees.

---

## Fase 2 — Revisão e Git

- [ ] Diff de cada worktree contra a base ref (staged, unstaged e arquivos não rastreados)
- [ ] Stage por arquivo
- [ ] Commit com a mensagem escrita pelo usuário
- [ ] Push com upstream; nunca force-push automático
- [ ] Annotate AI Diff: comentário em linha que vira prompt para o agente
- [ ] Conflito de merge: visualização básica
- [ ] Mensagem de commit sugerida por IA **(?)**

---

## Fase 3 — Produtividade

- [ ] Quick open de arquivos do worktree
- [ ] Jump palette entre worktrees, abas e sessões
- [ ] Integração com GitHub: PR e checks ligados ao worktree
- [ ] Integração com GitLab, Linear e Jira **(?)** ordem de prioridade
- [ ] Visualizador de arquivos e preview de markdown e imagens
- [ ] Arrastar arquivo para o terminal do agente
- [ ] Notificações do sistema e marcação de "não lido"

---

## Fase 4 — Avançado

- [ ] Daemon de PTY separado, para as sessões sobreviverem a fechar a janela
- [ ] Hooks de agentes (Claude Code, Codex) para estado mais preciso
- [ ] Orquestração multi-agente: runs, tarefas com dependências e mensagens
- [ ] CLI própria para o agente controlar o app
- [ ] Automações agendadas
- [ ] Execução remota por SSH
- [ ] Contas de agente com troca rápida e acompanhamento de uso
- [ ] Companion mobile
- [ ] Browser embutido com Design Mode

---

## Decisões em aberto

- **Daemon de PTY:** entra só na Fase 4. Proposta: na Fase 1 as sessões encerram ao fechar o app.
- **Local dos worktrees:** `<pasta-do-repo>/../<repo>-worktrees/<nome>` ou outro caminho gerenciado pelo app.
- **Pastas que não são git:** entram em qual fase?
- **Idioma da interface:** só inglês ou também pt-BR?
- **Identidade do projeto:** nome do pacote e identificador do Tauri, e licença.
