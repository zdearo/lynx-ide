# TODO — Lynx (`lynx-ide`)

Lista zerada. As tarefas entram sob demanda, uma frente por vez, e a ordem de execução fica na própria frente. Contexto, princípios e convenções estão em [AGENTS.md](./AGENTS.md). O histórico anterior (Fases 0 a 4) continua no git.

Legenda: `[ ]` pendente · `[x]` feito · **(?)** decisão em aberto.

---

## Bots

Assistentes persistentes que a pessoa conversa dentro do Lynx. O cérebro é o próprio Lynx, que fala com os provedores direto; as CLIs entram só como workers nos worktrees. Cada bot pode lançar agentes nos worktrees quando precisar, usar o browser embutido e operar o computador. Entram na barra primária, logo abaixo de "Projects".

- [ ] Modelo de execução: local. Decidido: o harness é o próprio Lynx, em Rust, dentro do app. Nuvem fica para depois
- [ ] Harness próprio: loop de chamada de modelo e adaptador por provedor. O cérebro não é uma CLI; as CLIs entram só como workers nos worktrees
- [x] Ponto de entrada: view "Bots" na barra primária, logo abaixo de "Projects" (`navigation-views.tsx`)
- [x] Segunda sidebar: lista de bots, e sem bot só o aviso. Criar abre um wizard em quatro etapas (nome e cor, motor e modelo, instruções, revisão), como um onboarding
- [x] Conversa com o bot escolhido: o main vira o chat, com status de "pensando" e histórico em `conversation.json`. O CLI não guarda sessão, então cada resposta recebe a conversa recente no prompt
- [x] Equipes: um rótulo por bot, na seção Agentes, com os bots ordenados por equipe
- [x] Canais: seção própria na sidebar, membros escolhidos na criação, resposta pelo roteador (com @ para forçar), em sequência e em streaming. Fora do escopo ainda: excluir canal, editar membros e trocar a equipe depois de criada
- [x] Acabamento do bot: instruções editáveis na aba de detalhes, apagar o bot com confirmação (recusado enquanto ele responde), erros de resposta guardados na conversa e links `https` abertos no navegador
- [x] Resposta aparece enquanto o bot escreve (`stream-json`, evento `bot-reply-delta`), com Markdown renderizado nas mensagens do bot
- [x] Painel da direita na view Bots: abas "Bot details" e "Bot browser" (o browser do bot ainda é só o lugar, sem acesso)
- [x] Modelo de bot em pasta: `LYNX_HOME/bots/<id>/` com `bot.json` (nome, cor, provedor, modelo) e `INSTRUCTIONS.md`. Decidido com a pessoa; nada vai para o SQLite
- [ ] Bot lança agentes em worktrees, sob demanda. Decidido: um worker por vez agora; orquestração de vários workers fica para a Fase 4
- [ ] Bot usa o browser embutido, pelas mesmas ferramentas `browser_*` que os agentes já têm
- [ ] Bot com computer use, fora do browser. Escopo e limites a definir
- [ ] Política de aprovação. Decidido: sem perguntar, ler, pesquisar, navegar e editar dentro do worktree; pedir confirmação, commit, push, abrir PR, enviar mensagem ou e-mail e clicar fora do worktree; recusar, apagar fora do worktree e mexer em credenciais

### Provedores, em ordem

Pré-requisitos: o harness e o modelo de bot acima. Feito: as regras do `AGENTS.md` foram reescritas (princípio 3 e token de login), e a seção Bots está lá.

1. [ ] **ChatGPT (Sign in with ChatGPT).** PoC feita: `config.bots.chatgpt.enabled` na tela Bots, login OAuth com PKCE em loopback, registro dinâmico, verificação do ID token pelo JWKS da OpenAI, credenciais em arquivo `0600` fora do `config.json`, e sair com tentativa de revogação. Falta: renovar o access token (refresh), usar o plano na inferência pela Responses API, e testar o login de ponta a ponta. A conta gratuita cai na tela de upgrade e "permissão obrigatória indisponível", então é preciso uma conta Plus ou Pro.
2. [ ] **Grok (Sign in).** Hoje a xAI não tem caminho oficial para terceiros; só o OpenCode foi anunciado. Antes de codar, confirmar com a xAI. **(?)** Fallback por API key (`api.x.ai/v1`) se a xAI não liberar: a analisar.
3. [ ] **Claude pela CLI.** Spike feito: `bots_claude_test` roda `claude -p` na pasta do bot, com o prompt do bot no lugar do padrão, sem ferramentas, sem CLAUDE.md de cima, sem hooks, plugins e MCP do usuário. Só depois dos itens 1 e 2 e do resto do bot prontos, para decidir se vale a pena. Usar `claude` com o login do usuário como cérebro do bot fica na zona cinzenta dos termos da Anthropic (rotear créditos de plano em nome do usuário). Nunca usar o token do Claude Code como credencial do harness. **(?)** Logo da Claude na interface: decidir antes de publicar o Lynx.

**Decidido:** o Lynx começa como projeto pessoal, sem licença open source por enquanto. **(?)** Como o Lynx já publica instaladores em `downloads.zdearo.dev`, confirmar com a OpenAI se isso cabe em "projeto pessoal que roda localmente" antes de liberar o item 1.
