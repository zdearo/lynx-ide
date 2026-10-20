# TODO — Lynx (`lynx-ide`)

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
- [x] Criar em segundo plano, com status visível (`fetch` e `worktree add`)
- [x] Colocar os worktrees fora da pasta do repo, em `<repo>-worktrees/<nome>` **(?)**
- [x] Remover worktree e branch; recusa alterações não commitadas, a menos que seja forçado (a confirmação na interface vem na 1.5)
- [x] Preservar a branch quando tiver commits não mergeados (`git branch -d` e motivo devolvido ao frontend)
- [x] Testes com repositórios temporários: criar, listar, remover, preservar branch, nomes inválidos, duplicados e worktree apagado por fora

### 1.3 Terminais (PTY)

- [x] Criar sessão de PTY com `portable-pty`, no diretório do worktree (shell de login do usuário; o comando é digitado no shell, então a aba continua viva quando o agente sai)
- [x] Comandos IPC: criar, listar, snapshot, enviar input, redimensionar e fechar
- [x] Enviar ao frontend os eventos de saída e de término (tipados, com scrollback de 1 MiB no Rust para reanexar a view sem perder nem duplicar saída)
- [x] Suportar várias sessões por worktree
- [x] Renderizar no frontend com xterm.js e o addon de fit (`TerminalView`, montado na janela)
- [x] Escolher o comando ao criar a sessão: shell, um agente ligado em Agents (Claude Code, Codex, OpenCode e os outros nove, cada um com o comando do seu CLI) ou comando livre
- [x] Encerrar todos os PTYs ao fechar o app, sem deixar processo órfão (SIGHUP no grupo em primeiro plano, SIGKILL após 3 s para quem ignora)

### 1.4 Estado dos agentes (básico)

- [x] Marcar a sessão como "trabalhando" ou "ocioso" pela atividade de saída. A linha do agente na sidebar (`AgentRow`) já existe e só precisa dos novos estados
- [x] Ler o título OSC do terminal quando o agente o envia
- [x] Guardar o estado num único store no Rust e exibi-lo na sidebar, por worktree. Hoje o estado fica no backend, por sessão (`agents/agent_monitor.rs`), vai em `TerminalInfo` e a sidebar agrupa por worktree
- [x] Indicar quando uma sessão termina (trabalhando → ocioso). Com sinal do agente (hook ou título de repouso) vira "concluído" até alguém olhar a sessão; sem sinal, volta a "ocioso"

### 1.5 Interface base

- [x] Layout com sidebar (repos → worktrees) e área principal com abas de terminal. Em janelas com menos de 900 px a sidebar vira um drawer
- [x] Componentes shadcn: Button, Dialog, AlertDialog, Input, Select, ScrollArea, Sheet, Sonner, DropdownMenu e Tooltip
- [x] Diálogo "Novo worktree" (nome e base). Fecha na hora, mostra uma linha em progresso na sidebar e reabre com o motivo se o Git recusar
- [x] Atalhos básicos: novo terminal, fechar aba, novo worktree e alternar sidebar (Cmd no macOS; Ctrl+Shift nos demais, para não roubar teclas do shell)
- [x] Adicionar projeto ("Add project"): abrir um repositório existente ou criar um vazio (`git init`) numa pasta escolhida
- [x] Foco do terminal: clicar na área do shell focaliza o xterm (sessões ocultas saem do layout, então não recebem o clique)
- [x] Agrupamento de agentes por worktree: cabeçalho "Agents" recolhível, com até 3 badges de status e `+X` para o resto
- [x] Temas do terminal: 14 temas (Lynx dark, Light, Solarized, Dracula, Nord, Catppuccin, Gruvbox, One Dark, Tokyo Night, Monokai). O tema dá o fundo; o texto segue o tema ou usa a paleta de outro

### Critério de pronto da Fase 1

Abrir um repo, criar dois worktrees, rodar um agente em cada um e ver os dois estados na sidebar. Fechar e reabrir o app sem perder a lista de repos e worktrees.

---

### Pastas de projetos

- [x] Pastas aninhadas no painel Projects (ex.: Work > Client X > projeto): criar (no topo ou dentro de outra), renomear, mover, excluir (o conteúdo sobe um nível) e recolher; o recolhido fica salvo
- [x] Mover repo ou pasta pelo menu, num diálogo com a árvore; uma pasta não entra em si mesma nem em descendente (checado na interface e no backend)
- [x] Pasta vinculada a um diretório do disco: os subdiretórios viram pastas e o primeiro repo git de cada caminho vira projeto; segue o disco (varredura ao abrir, a cada 60 s, ao focar e por "Rescan folder"), marca "Not found" o que sumiu e desvincula sem perder nada (ver "Pastas vinculadas" no AGENTS.md)
  - [ ] Vigiar o diretório com `notify` em vez de varrer a cada 60 s, se o polling pesar com vínculos muito grandes
  - [ ] Desfazer a exclusão de um projeto achado pela varredura sem desvincular a pasta (hoje só desvincular e vincular de novo limpa `linked_exclusions`)
- [ ] Arrastar e soltar repos e pastas na sidebar (hoje só pelo menu "Mover para pasta")
- [ ] Abrir um repositório já dentro de uma pasta, e criar worktree a partir da pasta
- [ ] Ordenação manual dentro de uma pasta (hoje: pastas por nome, repos pela ordem em que foram adicionados)
- [ ] Lembrar o recolhido de cada repo (hoje só o das pastas é salvo)
- [x] Pasta ou repo recolhido mostra um resumo à direita: projetos ou worktrees escondidos e quantos agentes estão rodando dentro (ponto verde + número)

### Pendências da Fase 1

- [x] Abrir e editar arquivos: clique seleciona, duplo clique (ou Enter) abre no editor CodeMirror 6, que ocupa a área central; os arquivos abertos ficam numa faixa abaixo das abas de terminal. Salva com Cmd/Ctrl+S, com detecção de mudança no disco (hash) e aviso de conflito. Realce para JS/TS/JSX/TSX, JSON, CSS, HTML, Markdown, Python e Rust
- [x] Aba Changes no painel da direita: lista de arquivos alterados (conflitos, staged, changes, untracked), diff do HEAD com a árvore de trabalho (Unified ou Split), aba de arquivo temporária com um clique e permanente com duplo clique, e botões Edit/Diff no topo do arquivo
- [x] Changes: descartar (com confirmação), contagem de linhas (+/−) por arquivo, diff por commit e por branch, comparar staged e unstaged, amend, pull/sync, force push com lease e arquivos de cada commit
- [ ] Editor: busca em todos os arquivos, desfazer ao reabrir, abas de arquivo fixas/em prévia, abrir imagem e arquivo grande em modo leitura, salvar automático opcional
- [ ] LSP: ponte em Rust (política A+D: servidor vivo só com o worktree em foco e um editor da linguagem aberto, para após ~10 min; limite LRU de segurança; servidores instalados pelo usuário) e `@codemirror/lsp-client` no editor
- [x] Vigia de arquivos: o explorador e o Changes se atualizam sozinhos quando um agente escreve no worktree da tela (`src-tauri/src/watching/`); a releitura de 4 s virou uma rede de segurança de 30 s
- [ ] Vigia: recarregar um arquivo aberto no editor quando ele muda no disco (hoje só o aviso de conflito ao salvar), e vigiar também as pastas ignoradas pelo Git sob demanda
- [ ] Explorador de arquivos: arrastar um item para o terminal
- [ ] Painel da direita: largura ajustável e novas abas (busca, source control)

- [ ] Exportar um JSON Schema do `config.json` para autocompletar no editor
- [x] Renomear o crate, o pacote, o binário e o identificador do bundle para `lynx-ide`. O produto segue Lynx (`productName`). Feito: `Cargo.toml`, `package.json`, `tauri.conf.json` (`dev.zdearo.lynx-ide`) e a documentação. Efeito: a pasta de dados do app muda com o identificador, então o app começa vazio nesse caminho; quem já usava a versão antiga precisa adicionar os repositórios de novo
  - [ ] Renomear a pasta do repositório no disco e o remoto no GitHub. Fica fora do código: a pasta muda o caminho da sessão do Claude Code, então faço só com o seu aval

- [ ] Decidir com o dono do projeto quais views entram na barra primária depois de "Projects"

- [x] Configurações em `~/.lynx/config.json`, com tela de settings em duas seções: Terminal (abas Text, Appearance e Behavior: fonte, tamanho, scrollback, tema e cores do texto, shell instalado, cópia ao soltar, sino, fechamento de abas) e Worktrees (pasta dos worktrees, com seletor)

- [ ] Verificar a interface na janela real do Tauri (hoje só foi exercitada no Chrome headless com o backend simulado, mais os testes do backend em Rust)
- [ ] Terminais ocultos continuam montados (mantêm tela e rolagem). Medir memória com 8 a 10 agentes e decidir se vale desmontar os inativos
- [ ] Windows e macOS: testar PTY, atalhos e seletor de pasta

---

## Fase 1b — Agentes

Depois da Fase 1. A Fase 1 entrega os estados genéricos (trabalhando, ocioso e encerrado), detectados pela saída e pelo título OSC. Esta fase traz o que depende de conhecer o agente: "precisa de você" e a configuração dos agentes.

- [x] Registro de harnesses: os 12 são reconhecidos pelo comando de lançamento (`agents/harness.rs`) e pelo processo em primeiro plano, com o nome do programa ou do script que um runtime executa (`harness_of_job`). Padrão de título por harness em `agents/agent_title.rs`: Claude (spinner e `✳`), OpenCode (`OC |`), Pi e OMP (marcador `π`, conferido no binário do OMP 18.8.2), e os demais pelo nome junto de uma palavra de estado. Os padrões dos demais vêm do Orca e ainda não foram vistos numa CLI real
- [x] Hooks dos agentes para o estado preciso e para "precisa de você". Instalar só com confirmação do usuário; nunca alterar a configuração do agente sem pedir (movido da Fase 4). Implementados para os 12: Claude Code (`claude_hooks.rs`), Codex (`codex_hooks.rs`), OpenCode (`opencode_hooks.rs`); os demais pela camada genérica (`json_hook_file.rs`, `hook_layouts.rs`, `kiro_hooks.rs`, `plugin_file.rs`, `pi_hooks.rs`)
  - Validados: Claude Code (usado pelo dono) e Codex (`hooks/list` do app-server 0.160.1). OpenCode só com relé falso, sem a CLI
  - [x] Grok: `grok inspect` lê os 9 eventos do arquivo do Lynx (33 hooks com ele, 24 sem), e `SessionStart`, `Stop` e `SessionEnd` chegam ao relé com `hook_event_name`. Sessão sem prompt, com a CLI 1.0.41
  - [x] OMP: a extensão carrega com `-e`, e `session_start` chega ao relé. Sessão sem prompt, com a CLI 18.8.2
  - [ ] Turnos com prompt no Grok (`UserPromptSubmit`, `PreToolUse`) e no OMP (`agent_start`, `agent_end`). Precisam de uma chamada de modelo na sua conta: aguardando sua autorização
  - [ ] Antigravity (`agy` 1.3.5): `PreInvocation`, `PreToolUse` e a resposta `decision: ask`. Só dispara com um prompt, e a pasta de configuração não tem variável de ambiente, então testar exige usar o `~/.gemini` real
  - [ ] Cursor, GitHub Copilot, Droid, Kiro, Qwen Code e Pi: validar quando forem instalados. Hoje valem só pela documentação e pelo código do Orca
  - [ ] Cursor: confirmar `{"permission":"ask"}` em `preToolUse`. O Orca responde `allow`; o Lynx pede `ask` para não aprovar ferramentas sozinho
  - [ ] Qwen Code e Droid: confirmar que o `SessionStart` traz o campo `source`, que o mapeamento do Claude exige
- [x] Estado "precisa de você" pelos hooks, pelo título e pelo primeiro plano. Aparece na sidebar (ícone de pergunta em `AgentRow`) e no resumo das pastas (`collapsed-summary.ts`, com o ícone de pergunta e a contagem)
- [x] Detectar agente digitado à mão pelo processo em primeiro plano: no Unix, o monitor lê o `argv` do grupo que tem o terminal; no Windows, o que o shell iniciou, pela árvore de processos (`terminal/foreground_job.rs`). Vira agente quando o programa, ou o script de um runtime, é um harness. Vale para Codex, OpenCode e sessões sem hooks
  - [ ] Detecção pelo primeiro plano no Windows: implementada e com um teste num `cmd.exe` real (`windows_foreground_tests.rs`). Falta rodar `cargo test` numa máquina Windows e confirmar
- [x] Agente sem evento de fim volta a shell quando o job sai do primeiro plano, depois de ter sido visto rodando. Vale para Antigravity, Cursor e Kiro, e para qualquer sessão. Testado com um CLI de mentira num shell real (bash)
- [x] Kiro sem arquivo de agente: ligar falha com aviso (`NoAgentFiles`), e o agente padrão do Kiro não é coberto, porque ele não tem configuração global de hooks
- [x] Seção de agentes na configuração: a lista (encontrados no PATH com uma alavanca, e os suportados não encontrados só informativos). Cada agente ligado ganha um interruptor de argumentos de lançamento, e o campo aparece só com ele ligado (`agents.launchArguments`). O modo de lançamento `Manual` ou `YOLO` vale para todos (`agents.launchMode`); YOLO pede confirmação e acrescenta a flag de cada agente
  - [ ] Confirmar as flags YOLO que vêm só da documentação, numa CLI instalada: Cursor (`--force`), Copilot (`--yolo`), Droid (`--skip-permissions-unsafe`), Kiro (`--trust-all-tools`, depois de `chat`) e Qwen Code (`--yolo`). As de Claude Code, Codex, Antigravity, Grok, OpenCode e OMP foram lidas no `--help` das CLIs instaladas aqui
  - [ ] Pi: não encontrei flag de YOLO na documentação; hoje abre sempre no modo manual
  - [ ] Codex: a flag YOLO também desliga o sandbox (`--dangerously-bypass-approvals-and-sandbox`), não só as aprovações. Decidir se o modo YOLO deve usar outra combinação
- [x] Ícones de marca dos 12 agentes, na lista de Agents, na sidebar e no menu de novo terminal (`AgentBrandIcon.tsx`). Cores originais, sem placa; o OMP usa o `icon.svg` do próprio repositório

Notificações de término e de "precisa de você" (Fase 3) dependem desta fase.

---

## Fase 2 — Revisão e Git

- [x] Diff de cada worktree contra a base ref (staged, unstaged e arquivos não rastreados)
- [x] Stage por arquivo
- [x] Commit com a mensagem escrita pelo usuário
- [x] Push com upstream; force push só com lease e depois de um segundo clique
- [ ] Annotate AI Diff: comentário em linha que vira prompt para o agente
- [ ] Conflito de merge: visualização básica
- [x] Mensagem de commit sugerida por IA, com o CLI do Claude Code, do Codex ou do OpenCode instalado (agente e estilo na aba Git das configurações; sem escolha de modelo ainda)
- [x] Fix with AI: entrega a um agente o erro de um commit rejeitado por hook, ou os checks que falharam no PR
- [x] Amend, descartar, pull/sync e arquivos de cada commit (ver Pendências da Fase 1)

---

## Fase 3 — Produtividade

- [x] Rodapé de status (24px), começando depois da barra de atividades e por baixo do painel de projetos, com o status do LSP à direita (vazio até o bridge existir)
- [ ] LSP: bridge no Rust que sobe e desce os servidores pela regra A+D (10 minutos, worktree em foco) e alimenta `lsp-status-store.ts`, que o rodapé já lê
- [ ] Limites de uso dos agentes no lado esquerdo do rodapé (acompanhamento de uso da Fase 4)

- [ ] Quick open de arquivos do worktree
- [ ] Jump palette entre worktrees, abas e sessões
- [x] Integração com GitHub: PR e checks ligados ao worktree (aba Pull request, via `gh`): criar, mesclar/fechar/reabrir, auto-merge, discussão e respostas, log dos checks e Fix with AI
- [x] Limpar o worktree depois do merge: com o PR mesclado, a aba Pull request oferece remover o worktree e o branch (recusa mudança não commitada; só apaga o branch se nada existe só nele)
- [ ] Limpar também o branch remoto e oferecer a limpeza sozinha, logo depois de mesclar pelo Lynx
- [ ] PRs empilhados (acompanhar e reordenar a pilha; abrir um PR sobre outro branch já funciona)
- [ ] Integração com GitLab (`glab`), Linear e Jira **(?)** ordem de prioridade
- [ ] Visualizador de arquivos e preview de markdown e imagens
- [ ] Arrastar arquivo para o terminal do agente
- [x] Notificações do sistema (agente concluído e agente precisa de você, só com a janela fora de frente, ligadas e desligadas em Settings) e marcação de "não lido" (o `done` do agente)
- [ ] Notificações: clicar leva ao worktree e à aba do agente; contador na janela ou no ícone; som

---

## Fase 4 — Avançado

- [ ] Daemon de PTY separado, para as sessões sobreviverem a fechar a janela
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
- **Projeto vazio sem commit:** o worktree precisa de uma base, e um repositório recém-criado não tem nenhuma. Opções: commit inicial vazio (exige identidade git configurada), aceitar o HEAD sem commit, ou desabilitar a criação de worktree nesse caso.
- **Tema claro da interface:** adiado, fora da Fase 1 (hoje só escuro, por decisão do shape).
- **Pastas que não são git:** entram em qual fase?
- **Idioma da interface:** só inglês ou também pt-BR?
- **Identidade do projeto:** nome do pacote e identificador do Tauri, e licença.
