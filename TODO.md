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

- [x] Escopo reduzido: suporte total a Claude Code, Codex, OpenCode e OMP. Os demais agentes (Antigravity, Cursor, Copilot, Droid, Grok, Kiro, Qwen Code e Pi) saíram do Lynx e rodam como shell ou comando personalizado. Registro de harnesses: os 4 são reconhecidos pelo comando de lançamento (`agents/harness.rs`) e pelo processo em primeiro plano, com o nome do programa ou do script que um runtime executa (`harness_of_job`). Padrão de título por harness em `agents/agent_title.rs`: Claude (spinner e `✳`), OpenCode (`OC |`), Pi e OMP (marcador `π`, conferido no binário do OMP 18.8.2), e os demais pelo nome junto de uma palavra de estado. Os padrões dos demais vêm do Orca e ainda não foram vistos numa CLI real
- [x] Hooks dos agentes para o estado preciso e para "precisa de você". Instalar só com confirmação do usuário; nunca alterar a configuração do agente sem pedir (movido da Fase 4). Implementados para os 12: Claude Code (`claude_hooks.rs`), Codex (`codex_hooks.rs`), OpenCode (`opencode_hooks.rs`); os demais pela camada genérica (`json_hook_file.rs`, `hook_layouts.rs`, `kiro_hooks.rs`, `plugin_file.rs`, `pi_hooks.rs`)
  - Validados: Claude Code (usado pelo dono) e Codex (`hooks/list` do app-server 0.160.1). OpenCode só com relé falso, sem a CLI
  - [x] OMP: a extensão carrega com `-e`, e `session_start` chega ao relé. Sessão sem prompt, com a CLI 18.8.2
- [x] Estado "precisa de você" pelos hooks, pelo título e pelo primeiro plano. Aparece na sidebar (ícone de pergunta em `AgentRow`) e no resumo das pastas (`collapsed-summary.ts`, com o ícone de pergunta e a contagem)
- [x] Detectar agente digitado à mão pelo processo em primeiro plano: no Unix, o monitor lê o `argv` do grupo que tem o terminal; no Windows, o que o shell iniciou, pela árvore de processos (`terminal/foreground_job.rs`). Vira agente quando o programa, ou o script de um runtime, é um harness. Vale para Codex, OpenCode e sessões sem hooks
  - [ ] Detecção pelo primeiro plano no Windows: implementada e com um teste num `cmd.exe` real (`windows_foreground_tests.rs`). Falta rodar `cargo test` numa máquina Windows e confirmar
- [x] Agente sem evento de fim volta a shell quando o job sai do primeiro plano, depois de ter sido visto rodando. Vale para Antigravity, Cursor e Kiro, e para qualquer sessão. Testado com um CLI de mentira num shell real (bash)
- [x] Seção de agentes na configuração: a lista (encontrados no PATH com uma alavanca, e os suportados não encontrados só informativos). Cada agente ligado ganha um interruptor de argumentos de lançamento, e o campo aparece só com ele ligado (`agents.launchArguments`). O modo de lançamento `Manual` ou `YOLO` vale para todos (`agents.launchMode`); YOLO pede confirmação e acrescenta a flag de cada agente
- [x] Ícones de marca dos 4 agentes, na lista de Agents, na sidebar e no menu de novo terminal (`AgentBrandIcon.tsx`). Cores originais, sem placa; o OMP usa o `icon.svg` do próprio repositório
  - [ ] Codex: a flag YOLO também desliga o sandbox (`--dangerously-bypass-approvals-and-sandbox`), não só as aprovações. Decidir se o modo YOLO deve usar outra combinação

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
- [x] Limites dos planos no lado esquerdo do rodapé: Codex (`codex app-server`) e Claude Code (`get_usage`), com popover por janela
- [x] Limites do plano do Claude Code pelo `get_usage` do protocolo stream-json (o mesmo do Agent SDK), sem tocar no `statusLine` do usuário
- [x] Limites: configurações (visível, planos, usado ou restante, compacto ou detalhado, limiares de aviso) e popover com o ritmo estimado
- [ ] Limites de outros provedores (OpenCode e OMP), cada um pelo protocolo do próprio CLI, se houver um

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

## Browser embutido (agentes)

Decidido em vez de um browser no estilo Orca: o Chromium (CEF) roda num processo à parte (`lynx-browser-host`), a página é desenhada numa view GTK nativa sobre o webview e os agentes a usam por MCP. Ver a seção Browser do AGENTS.md.

- [x] Motor em processo separado, com protocolo próprio (socket Unix + frames em memória compartilhada) e `bun run browser:engine`
- [x] View nativa no Linux (Wayland), com barra, digitação com acentos, HiDPI, rolagem e redimensionamento
- [x] Destacar a página para uma janela própria e devolvê-la, sem recarregar
- [x] Ferramentas para o agente por MCP (`lynx --mcp`): snapshot com refs, navegar, clicar, digitar, teclas, rolar, esperar, screenshot, escolher opção, console e rede
- [x] MCP instalado e removido junto dos hooks (Claude Code, Codex e OpenCode), sem diálogo de confirmação
- [x] Limites do usuário: somente leitura ou desligado, e lista de sites
- [x] O agente visível na página: cursor com a marca do agente, rótulo da ação, anel de carregamento e moldura; o browser não abre sozinho
- [x] O que ninguém responde, o host responde: diálogos JS, permissões, downloads, seletor de arquivo e menu de contexto
- [x] Download do motor de dentro do app, com manifesto, SHA-256, versão de protocolo e pacote gerado por `--bundle` (workflow `browser-engine.yml`)
- [ ] **Publicar a primeira versão do motor no R2** (o endereço padrão do manifesto é `downloads.zdearo.dev/engine/`; o workflow `browser-engine.yml` ainda não publicou nada, só roda com `publish` marcado)
- [ ] Sandbox do Chromium (hoje `no_sandbox`) e perfil persistente (login salvo)
- [ ] Várias abas, ou uma página por worktree; hoje há uma só e dois agentes disputam a mesma
- [ ] Cópia e cola, Tab preso na página, atalhos do Lynx com a página em foco e o `<select>` nativo para o usuário
- [ ] iframes no snapshot; hover, arrastar e enviar arquivo como ferramentas
- [ ] MCP nos demais agentes (só Claude Code, Codex e OpenCode hoje)
- [ ] Pré-autorizar as ferramentas `lynx` nos agentes, para não pedirem permissão a cada chamada (decisão do usuário)
- [x] Windows: transporte por named pipe, view Win32, overlay do agente em software, host sem console, instalador por renomeação, pacote `windows-x64` e job no workflow (compila e passa no clippy; **nunca rodou numa máquina Windows**)
- [ ] Testar o Windows numa máquina real: janela filha sobre o WebView2, foco, teclado e IME, DPI, destacar a página, pacote e instalação
- [ ] macOS: o motor vira um `.app` com o framework e cinco helpers, assinatura e notarização, view `NSView` e entrada com `NSTextInputClient`

---

## Fase 4 — Avançado

- [ ] Daemon de PTY separado, para as sessões sobreviverem a fechar a janela (Linux e Windows; macOS fica fora até haver máquina). Referência: o daemon do Orca (`src/main/daemon`), lido por amostragem
  - Pré-requisito feito: `hub.json` (porta e token estáveis para hooks e MCP) e instância única do app. Testes de PTY no `windows-latest` (`windows_terminal_tests.rs`) já passam
  - [x] **D1, o núcleo** (`crates/pty-daemon`): sessões por socket, reanexar com snapshot e saída ao vivo, trava de arquivo (um daemon por pasta), saída em lotes de 4 ms (16 KB no máximo), decodificador UTF-8 de fluxo, cliente que não lê não segura a sessão, encerrar mata o grupo do programa, e sai sozinho sem sessões. Testes no Linux; o Windows roda os testes de `cmd.exe` no CI. Desvios do plano: o daemon é o próprio app com `--pty-daemon` (um executável só, sem `externalBin`, e o caminho do AppImage é o do relé), e feitos depois o token do socket e a pasta privada (0700); ficou para depois o escape de cgroup
  - [x] **D2, o app vira cliente**, atrás de `terminal.ptyDaemon` (desligado por padrão): só as sessões novas passam pelo daemon, e se ele não sobe o app roda a sessão no próprio processo. Rodapé mostra o estado (sem daemon, conectado com N sessões, indisponível). Ao abrir, o app encerra as sessões que uma execução anterior deixou no daemon, porque nenhuma janela as mostra. Por enquanto fechar o app ainda encerra as sessões do daemon (`close_all`); manter as sessões é a D3
  - [ ] Verificar no Windows: o daemon se desacopla do app (`DETACHED_PROCESS`), o `cmd.exe` no daemon e a resposta ao cursor do ConPTY. Só há verificação de tipos (clippy) daqui
  - [x] **Retomada** (independente do daemon): o id da conversa vem do hook do Claude (`session_id`) e do OpenCode (`session.created`); o Codex deve trazer o mesmo campo, a confirmar numa captura; o OMP não traz id e fica de fora. Fica em `terminal_sessions` (`app.db`) e o botão reinicia a conversa com o mesmo modo de lançamento. Cobre reboot, porque a conversa está em disco no próprio agente
  - [x] **D3, ciclo de vida** (parte): ao abrir, o app reanexa as sessões que o daemon ainda roda e marca como encerradas as que ele perdeu; ao fechar com sessões do daemon, pergunta se mantém (padrão) ou encerra; reiniciar pelo update encerra tudo antes da nova versão; o socket exige um token em arquivo privado, e a pasta do daemon é 0700. No Windows o daemon roda de uma cópia em `LYNX_HOME/daemon/bin/`, para o instalador trocar o executável do app
  - [ ] **D3, histórico em disco** (banco próprio do daemon, `terminal-history.db`): gravação em lote no daemon, limite por sessão, apaga ao fechar a sessão de propósito, e restauração do histórico de sessões encerradas. Não foi feito: a retomada cobre a conversa, e isso é só a tela anterior
  - [ ] **Próximas etapas do daemon, em ordem de execução.** Feitas, com o que ficou de fora em cada uma; o histórico de saída em disco segue pendente.
    - [x] **1. Atualizar sem matar os agentes.** `restart_to_update` deixa as sessões do daemon de pé e a versão nova reanexa (mesmo protocolo, `daemon/pty-v1`). Diálogo de reinício diz o que fecha, conforme a configuração. Testes: a sessão segue viva depois de `quit` mantendo, e a nova instância a reanexa. Um daemon de outra versão (arquivo `version` diferente ou ausente) é trocado no próximo início do app, mas só quando não tem sessões: com sessões ele fica, e as funções novas dele respondem com erro até elas acabarem. Mudança de protocolo (`pty-v2`) ainda não existe, e nele o daemon antigo continuaria sozinho com as sessões dele
    - [x] **2. Medição por sessão.** Requisição `Usage` no daemon: memória (RSS) e tempo de CPU da árvore de cada sessão, lida só quando pedida (`sysinfo`). O rodapé mostra a memória total ao passar o mouse, sem varredura em segundo plano. O tempo de CPU já vem no protocolo, mas nenhuma tela o mostra ainda. Memória de processos que dividem páginas conta uma vez por processo
    - [x] **3. Base para orquestrar.** Banco do daemon `terminal-history.db`: sessões que um comando de linha iniciou (`lynx-ide --pty-session <pasta> [comando]`, ainda não a CLI do item de Fase 4) e eventos de agente que chegaram com o app fechado. O relé cai para o daemon quando o hub do app não aceita o evento; ao abrir, o app mostra as sessões de linha de comando, entrega os eventos guardados ao monitor e os remove do daemon. Testes: sessão criada com o app fechado aparece ao abrir com a saída; evento deixado com o app fechado define o id da conversa. Não feito: o histórico de saída em disco (D3), que usa a mesma base
    - [x] **4. Hibernação (manual).** Clique direito na aba, "Sleep terminal": o daemon congela o grupo do programa em primeiro plano (`SIGSTOP`) e deixa o shell rodando, que vê a parada e devolve o terminal a si mesmo; a aba mostra "sleeping". Digitar ou reanexar acorda: com controle de tarefas no shell, o daemon digita `fg` e espera o programa ter o terminal antes de entregar a digitação; sem isso, `SIGCONT`. Encerrar acorda antes, para o SIGHUP chegar. Recusada, com o motivo, enquanto o agente trabalha, enquanto uma ferramenta do browser roda para a sessão, ou fora do daemon. Só Unix: no Windows é recusada. Não feito: a automática, que depende de medir a memória com 8 a 10 agentes
    - [ ] Depois: breaker de crescimento de processos.
- [ ] Orquestração multi-agente: runs, tarefas com dependências e mensagens
  - [x] Fatia 0: o backend emite `terminal-created` para toda sessão que inicia (a lista de abas a recebe, sem trocar a aba ativa), e a linha de comando de cada agente é montada no Rust (`agent_launch_line`), com os mesmos casos de `agent-presets.test.ts`
  - [x] Fatia 1: Run, Task e Dispatch no SQLite (migração 9), serviço com as regras (aprovação, dependências, orçamento: simultâneos, total e agentes permitidos, YOLO só se aprovado), roteador `lynx_*` no hub e as ferramentas `lynx_run_start`, `lynx_task_create`, `lynx_worker_start`, `lynx_worker_read` e `lynx_worker_stop`. A aprovação do orçamento é um comando IPC (`approve_run`), ainda sem tela. Não feito: a tabela de mensagens e `lynx_report`/`lynx_check` (fatia 2), o teto de uso e o tempo máximo do orçamento (fatia 2)
  - [ ] Fatia 2: `lynx_report`, `lynx_check`, aprovação do orçamento na UI e fallback por status
  - [ ] Fatia 3: view Orchestration e notificações
- [ ] CLI própria para o agente controlar o app
- [ ] Automações agendadas
- [ ] Execução remota por SSH
- [ ] Contas de agente com troca rápida e acompanhamento de uso
- [ ] Companion mobile
- [ ] Browser embutido: Design Mode (selecionar um elemento da página e mandar ao agente)

---

## Decisões em aberto

- **Daemon de PTY:** decidido; a sequência D1, D2, retomada e D3 está na Fase 4. Até lá as sessões encerram ao fechar o app.
- **Local dos worktrees:** `<pasta-do-repo>/../<repo>-worktrees/<nome>` ou outro caminho gerenciado pelo app.
- **Projeto vazio sem commit:** o worktree precisa de uma base, e um repositório recém-criado não tem nenhuma. Opções: commit inicial vazio (exige identidade git configurada), aceitar o HEAD sem commit, ou desabilitar a criação de worktree nesse caso.
- **Tema claro da interface:** adiado, fora da Fase 1 (hoje só escuro, por decisão do shape).
- **Pastas que não são git:** entram em qual fase?
- **Idioma da interface:** só inglês ou também pt-BR?
- **Identidade do projeto:** nome do pacote e identificador do Tauri, e licença.
