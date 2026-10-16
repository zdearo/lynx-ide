# AGENTS.md

Contexto do projeto para agentes de IA (Claude Code, Codex e similares) que trabalham aqui. Leia antes de mexer no código. As tarefas e a ordem de execução estão em [TODO.md](./TODO.md).

## O projeto

**agent-ade** (nome do produto: **Lynx**, ver `PRODUCT.md`) é um ADE (Agentic Development Environment) desktop. Ele roda vários agentes de código em paralelo (Claude Code, Codex, OpenCode e outros CLIs), cada um isolado no seu próprio `git worktree`, com terminais, estado e revisão reunidos num lugar só.

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

- `src/`: frontend React. Primitivos shadcn em `src/components/ui/`; o alias `@/` aponta para `src/`.
  - `workspace/`: o painel Projects, seções de repo, linhas de worktree, diálogos de worktree e `workspace-store.ts` (estado de repos, pastas e worktrees). "Add project" (`AddProjectDialog.tsx`) oferece dois cards: abrir um repositório que já existe (o seletor de pastas) ou criar um projeto vazio, que pede nome e a pasta de destino antes de criar (`create_project` no backend: `git init` numa pasta nova).
  - Árvore de projetos: `project-tree.ts` monta a árvore (pastas primeiro, ordenadas por nome; depois os repos na ordem em que foram adicionados; uma pasta ou repo com pai sumido, ou preso num ciclo, sobe para o topo). `ProjectTree.tsx` desenha um nível e se chama de novo dentro de cada pasta; `FolderSection.tsx` é a pasta, `RepoSection.tsx` é o repo. Mover usa o `MoveDialog.tsx` (escolha em árvore, com setas; uma pasta não aparece como destino dela mesma nem de seus descendentes). Criar e renomear: `FolderDialog.tsx`; excluir: `DeleteFolderDialog.tsx`.
  - Pasta ou repo recolhido mostra à direita um resumo (`CollapsedSummary`, contagem em `collapsed-summary.ts`): projetos ou worktrees escondidos e, se houver, o ponto verde com os agentes rodando dentro. O texto completo vai para leitores de tela por `aria-describedby`, então o nome do cabeçalho continua sendo só "Collapse X" / "Expand X". As ações da linha aparecem com o mouse em cima ou com foco de teclado (`group-has-[:focus-visible]`); com foco de mouse elas não aparecem, para o resumo continuar visível depois de um clique.
  - Pasta do topo (`depth` 0 em `ProjectTree`) é rótulo de grupo: texto pequeno em caixa-alta, sem ícone, com `mt-3` separando um grupo do próximo. Subpasta tem ícone de pasta.
  - À esquerda de cada linha fica o tipo do item (pasta aberta ou fechada, ícone de repo). A seta de recolher fica à direita, dentro das ações que aparecem ao passar o mouse. Essas ações são `pointer-events-none` e cada botão delas é `pointer-events-auto`: assim a área da seta continua clicando no cabeçalho por baixo. Não tire isso sem testar o clique.
  - Todo worktree com agentes mantém o grupo deles visível na sidebar, aberto ou não. O cabeçalho do grupo é `workspace/AgentGroup.tsx`: a palavra "Agents" com o mesmo peso e cor do nome do repo; recolhido, mostra até 3 badges de status e um `+X` para o resto; aberto, só o título e as linhas. O cabeçalho não tem fundo nem cor próprios: só ganha o hover. `WorktreeGroup.tsx` é um bloco: o cartão (só fundo, sem borda, é a seleção), a linha do worktree (em destaque, sem fundo próprio) e os agentes dentro dele, com margem de 6px ao redor e 4px entre si. Só o agente aberto ganha um fundo próprio, mais claro que o cartão. Não pinte as linhas separadamente, de ponta a ponta: parecem duas seleções. Listas em `grid` precisam de `grid-cols-1`, senão os itens não encolhem e o `truncate` não funciona. Linha de agente: `workspace/AgentRow.tsx`: são as sessões iniciadas a partir de um agente conhecido (`isAgentCommand`); shells comuns ficam só na barra de abas. Cada linha mostra status, glifo, título (o mesmo da aba) e a idade desde que a sessão começou (`startedAt`, vindo do backend). Hoje o status só distingue rodando de encerrado; trabalhando/precisa de você/ocioso entram com a detecção de estado (item 1.4).
  - `terminal/`: abas, `TerminalView` (xterm), `TerminalStreamGate`, presets de agente e `terminal-store.ts`.
  - `navigation/`: a barra primária (`ActivityBar`, estilo VS Code) e o painel lateral da view ativa. Para criar uma view nova, adicione-a em `navigation-views.tsx` (ícone, rótulo e o componente do painel) e ao tipo `NavigationViewId`. Hoje só existe "Projects" (`workspace/ProjectsPanel.tsx`).
  - `right-panel/`: o painel da direita, com abas (hoje só Files). Aba nova entra em `right-panel-tabs.tsx`. Acoplado ao lado do terminal a partir de 1280 px; abaixo disso abre como drawer pela direita. Começa fechado; atalho Cmd/Ctrl+Shift+E.
  - `files/`: a aba Files. `explorer-store.ts` guarda a árvore por worktree (pastas carregadas sob demanda), `file-tree-rows.ts` transforma isso em linhas, `FilesPanel.tsx` e `FileRow.tsx` desenham (padrão de árvore ARIA, navegação por setas, Shift+F10 abre o menu), `file-icon-kind.ts` decide o ícone (nomes exatos, depois prefixos, depois extensão; o nome ganha da extensão, então `package.json` é pacote e não só JSON) e `FileTypeIcon.tsx` mapeia cada tipo para um ícone do Lucide (tipo novo: um valor em `FileIconKind`, as regras e uma entrada em `ICONS`), e `file-actions.ts` tem copiar caminho, enviar o caminho ao terminal (com aspas de shell) e abrir a pasta.
  - `config/` e `settings/`: o store da configuração e o modal de settings (seções Terminal e Worktrees; novas seções entram em `settings-sections.ts`).
  - `ui-copy.ts`: todos os textos da interface (inglês). `dialog-store.ts`: qual diálogo está aberto. `platform-shortcuts.ts` e `use-app-shortcuts.ts`: atalhos.
  - `dev/mock-tauri.ts`: backend simulado, só em desenvolvimento (`?mock=full` ou `?mock=empty` na URL).
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

## Interface

A direção visual é a família do Orca, com acabamento no nível de Orca, Linear e Raycast (ver `PRODUCT.md`). O contrato da direção está em `.impeccable/surfaces/`. Diferenças em relação ao Orca foram adiadas de propósito: faça o básico bem feito antes de divergir.

- Todo texto visível vem de `ui-copy.ts`. Não escreva strings soltas em componentes.
- Cores e tamanhos vêm dos tokens de `src/index.css`: `bg-live` (terminal rodando), `text-agent` (glifos de agente), `text-ui` (13px, nome do worktree), `text-nested` (12px, agentes sob o worktree) e `text-meta` (11px, idade). Nada de valores fixos. Tamanho de fonte novo precisa ser declarado em `src/index.css` e também em `src/lib/utils.ts` (o `cn` lê `text-*` sem isso como cor e descarta a classe quando uma cor vem depois). A única exceção é o tema do xterm, que desenha em canvas e não lê tokens oklch.
- O estado nunca depende só de cor: ponto cheio significa que há terminal rodando, ponto vazado que não há.
- Atalhos: Cmd no macOS e, nos demais, Ctrl+Shift para as letras que o shell usa (T/W/N/B) e Ctrl puro para as outras (configurações: Cmd/Ctrl+`,`). Eles são capturados na fase de captura da janela, antes do terminal.
- Toda ação por ícone usa `TooltipIconButton` (nome acessível e dica). Listas dentro de `ScrollArea` já têm `[&>div]:block!` para truncar com reticências.
- Abas de terminal: o "×" fica dentro da aba (uma `tablist` só pode conter abas) e o teclado fecha com `Delete` ou com o atalho.
- O contrato visual é verificado com `bun run e2e` (Chrome headless contra o backend simulado, inclui axe WCAG A/AA). Rode-o ao mexer na interface. Não capture a tela do monitor: a captura pode pegar outras janelas do usuário.

## Configuração

As preferências do usuário ficam em `~/.lynx/config.json` (`LYNX_HOME` troca a pasta), em JSON simples como o Claude Code e o OpenCode. A regra de divisão: **arquivo = o que o usuário escolhe** (fonte, shell, pasta dos worktrees, depois atalhos e agentes); **SQLite = o estado do app** (repos abertos, base de cada worktree). Segredos nunca vão no arquivo.

- Código em `src-tauri/src/config/`: `app_config.rs` (tipos, padrões, validação, leitura tolerante), `config_store.rs` (arquivo) e `config_events.rs` (vigia a cada 1 s e emite `config-changed`).
- O arquivo é do usuário e pode ser editado à mão. Leitura: um campo inválido é avisado e vira o padrão, sem derrubar o resto. Gravação: só vão as chaves que diferem do padrão, chaves desconhecidas e a ordem são preservadas, e a escrita é atômica. Um arquivo que não é JSON nunca é sobrescrito: a interface mostra o aviso e desabilita os campos.
- Para criar uma configuração nova: campo e padrão em `AppConfig`; leitura em `parse_config`, validação em `validate`, reset em `reset` e escrita em `apply`; o espelho do padrão em `src/config/config-store.ts`; o campo na tela (`src/settings/`) e um teste. O que usa o valor deve lê-lo quando precisa (como o shell e a pasta dos worktrees), para a mudança valer sem reiniciar.
- Telas de settings: a seção Terminal tem abas internas na horizontal (Text: fonte, tamanho, scrollback; Appearance: tema e cores do texto; Behavior: shell, cópia, sino, fechamento).
- Aparência: são duas escolhas. `colorScheme` é o tema e define o fundo. `textColorScheme` define a paleta de texto inteira (cor principal e as 16 ANSI): fica vazio para seguir o tema, ou aponta para outro tema. Os temas são o enum `ColorScheme` (Rust) e o mapa `SCHEMES` em `src/terminal/terminal-color-schemes.ts`: ao criar um tema, adicione nos dois lados, no `copy.settings.terminal.themes` e no teste de temas. Conferidas com fonte oficial: fundo e texto dos Catppuccin Latte e Frappé, e a paleta ANSI do Mocha. O resto (ANSI dos outros Catppuccin, Gruvbox, One Dark, Tokyo Night e Monokai) veio de referência de memória e ainda não foi conferido com a fonte oficial. Os campos estão em `src/settings/SettingFields.tsx`: `TextSetting` (com botão de escolher pasta opcional), `ChoiceSetting` (select com entrada "Custom…" opcional; a lógica pura fica em `choice-field.ts`), `StepperSetting` (mais e menos, para números com limite) e `SwitchSetting`. Shells instalados vêm de `list_shells` (`/etc/shells`, sem duplicar links). Não use `<input type="number">` para configuração.
- Comportamento do terminal: o sino só pisca a borda (`onBell` do xterm; som não está implementado). Copiar ao soltar o mouse usa `mouseup` no contêiner do terminal. Todo fechamento iniciado pelo usuário passa por `requestCloseTerminal` (`terminal-actions.ts`), para a confirmação valer na aba, no atalho e no `x` do agente. Fim de processo passa por `handleTerminalExit`.
- Uma camada por projeto (dentro do repositório) só pode existir com confirmação de confiança: configuração pode definir comandos, e abrir um repositório clonado não pode executar código dele.

## Arquivos

`src-tauri/src/files/` lista uma pasta por vez para o explorador. O frontend nunca manda um caminho absoluto: manda o worktree e um caminho relativo.

- O worktree precisa ter sido reportado pelo Git (`WorkspaceService::resolve_worktree`), e o caminho relativo é resolvido por `resolve_inside`, que recusa `..`, caminho absoluto, barra invertida e symlinks que saem da raiz. Qualquer comando novo que leia disco por pedido do frontend deve passar por esses dois.
- A listagem mostra tudo, menos `.git`; o que o Git ignora vem marcado (`git check-ignore`, um processo por pasta) e aparece esmaecido. Pastas com mais de 5000 itens são cortadas e avisam.
- O conteúdo dos arquivos ainda não é lido: ver e editar fica para a fase do editor.

## Persistência

O estado do app fica em SQLite (`rusqlite` com SQLite embutido), no arquivo `app.db` do diretório de dados. O acesso passa por `src-tauri/src/persistence/`.

- Mudança de schema é uma nova entrada no fim de `MIGRATIONS` em `schema_migrations.rs`. Nunca edite uma migração já lançada.
- Um banco criado por versão mais nova é recusado sem alteração; um arquivo que não é SQLite é guardado como `app.db.corrupt-<timestamp>`.
- O Git continua sendo a fonte da verdade sobre worktrees: o banco só guarda o que o Git não sabe (repos abertos, base ref de cada worktree).
- Pastas de projetos também ficam aqui (tabela `folders` e `repos.folder_id`), junto dos repos, para que o banco garanta que a pasta de um repo existe. Regras: uma pasta nunca fica dentro de si mesma nem de um descendente (`folder_hierarchy.rs`, checado no serviço antes de gravar); excluir uma pasta sobe o conteúdo um nível numa única transação e nunca remove nada do disco; o recolhido de cada pasta é guardado.

## Terminais

`src-tauri/src/terminal/` é dono dos PTYs; a UI só mostra e envia input.

- O backend guarda 1 MiB de scrollback por sessão e numera os chunks (`seq`). A view se anexa com `terminal_snapshot` e `TerminalStreamGate` (`src/terminal/`) junta o snapshot com os eventos ao vivo. Não escreva saída no xterm fora do gate.
- O comando de um agente é digitado num shell de login, nunca executado no lugar dele.
- Fontes: o terminal usa JetBrains Mono e Symbols Nerd Font Mono, embutidas (`terminal-appearance.ts`, `terminal-fonts.css`), para ser igual em qualquer máquina e mostrar os ícones de prompts e agentes. O terminal só é criado depois que as fontes de texto carregam: o xterm mede a célula ao abrir e mediria errado com uma fonte de reserva. Licenças em `THIRD_PARTY_NOTICES.md`.
- Padding nunca vai no elemento onde o xterm é aberto: o `FitAddon` mede o pai e não desconta o padding dele, então a última linha passa do limite. O padding fica num contêiner por fora (`TerminalView`). O `bun run e2e` verifica isso.
- Encerrar sessão: nunca envie sinal a um PID de sessão que já saiu (o PID pode ter sido reaproveitado). `TerminalSession::terminate` já faz essa checagem.
- Os testes de `terminal_manager_tests.rs` usam shells reais e só rodam em Unix. Os `seq` do contrato são `u32` porque o specta não exporta `u64` para TypeScript.

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
- **Contrato IPC:** os tipos de comandos e eventos são definidos só no Rust (`tauri-specta`) e o TypeScript é gerado em `src/bindings.ts`. Nunca edite esse arquivo à mão e não chame `invoke("...")` direto: use `commands.*` de `@/bindings`. Para criar um comando: derive `serde::Serialize` e `specta::Type` nos tipos, anote a função com `#[tauri::command]` e `#[specta::specta]`, registre em `src-tauri/src/ipc.rs` (`collect_commands!`) e rode `bun run bindings`. O teste `committed_bindings_match_rust_contract` falha se o arquivo ficar desatualizado. As versões de `specta` e `tauri-specta` estão fixadas com `=` porque ainda são release candidates: atualize-as juntas e deliberadamente.
- **Testes:** a lógica de Git é testada com repositórios temporários criados dentro do próprio teste (crate `tempfile`).
- **Commits:** só quando o dono do projeto pedir.

## Estado atual

- Fase 0 concluída. Fase 1: backend de worktrees, persistência e terminais prontos; interface mínima pronta (sidebar por repositório, abas de terminal, diálogos). Faltam o estado dos agentes (1.4) e a verificação na janela real do Tauri. Ver TODO.md.
- Ambiente de desenvolvimento: Arch Linux (Hyprland), Node 24 e Bun. O Rust padrão da máquina é nightly, mas o projeto usa a estável via `rust-toolchain.toml`. As bibliotecas GTK e WebKit 4.1 já estão instaladas.

## Comandos

Gerenciador de pacotes: Bun (`bun install`, lockfile `bun.lock`).

- `bun run tauri dev`: sobe o Vite e o app desktop com hot reload.
- `bun run tauri build`: gera o pacote de release. `bun run tauri build --no-bundle` só compila o binário.
- `bun run e2e`: sobe o Vite numa porta própria e roda `e2e/ui-smoke.mjs` no Chrome headless (precisa de Chrome; use `CHROME_PATH` se não estiver no caminho padrão).
- `bun run check`: typecheck, ESLint, Prettier (verificação) e Vitest do frontend. Rode antes de concluir uma tarefa no frontend.
- `bun run rust:check`: `cargo fmt --check` e `cargo clippy -D warnings`.
- `bun run rust:test`: testes do backend (inclui a checagem de sincronia dos bindings).
- `bun run bindings`: regenera `src/bindings.ts` a partir do Rust.
- `bun run format`: formata o projeto com Prettier.
- Adicionar componente shadcn: `bunx shadcn@latest add <componente>`.

`bun run tauri dev` abre uma janela na tela do usuário. Evite rodá-lo sem necessidade e nunca tire capturas de tela do monitor para validar a UI: a captura pode pegar outras janelas do usuário.

## Fora de escopo por enquanto

Orquestração multi-agente, CLI própria, automações agendadas, execução remota por SSH, companion mobile, browser embutido e pastas que não são repositórios git.
