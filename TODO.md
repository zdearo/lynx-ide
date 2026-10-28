# TODO — Lynx IDE

## Equipe (orquestração)

POC só com dados simulados (`?mock=full`, aba Team). Nada disso fala com agentes reais ainda. Referência de comportamento: o Maestri (skill `maestri-manager` e a [página do modo Maestro](https://themaestri.app/pt-br/docs/maestro)).

### Papéis

- **Orquestrador:** o único tipo de membro que pode ter filhos. Cria, dispensa, troca o papel, define a tarefa e o critério de pronto e pede e cobra os filhos diretos.
- **Filho direto de um orquestrador:** tem tarefa e critério de pronto, definidos pelo orquestrador.
- **Sem orquestrador (sem associação):** não tem tarefa nem critério de pronto. Só recebe conversa e responde. Não pede nada ao usuário. Fica fora da árvore (posição abaixo).
- Um membro não pode ter filhos sem ser orquestrador. Desligar o modo orquestrador com filhos é recusado.
- Posição na tela: o orquestrador do topo fica na linha de cima. Sem orquestrador, o membro fica na linha do par; sem par, uma linha abaixo do topo. Fora da árvore, ele só se move de lado.

### Autoridade

- O orquestrador manda só nos filhos diretos. Ele enxerga a árvore toda, mas para algo que precisa de um neto pede ao filho, que repassa.
- Um orquestrador não orquestra agente de outro orquestrador.
- Ligação nenhuma dá autoridade. Só a posição na árvore dá.

### Conversa

- Uma ligação entre dois cartões é um canal de mensagem. Vale entre cartões do mesmo nível (par lateral) e de cima para baixo (orquestrador e filho).
- Conversa não cria, não dispensa, não define tarefa e não dá veredito.
- Par lateral é permissão de conversa, não de autoridade.

### Dispensar

- Dispensar um orquestrador solta os filhos: eles ficam sem orquestrador, sem tarefa e fora da árvore.
- Dispensar um membro comum também remove os pares dele.

### Objetivo da run

- O objetivo da run ("Auth refactor") não é tarefa de membro. Fica acima da equipe, e o orquestrador do topo não tem tarefa.

### Runs

- Cada árvore é uma run, com objetivo próprio e equipe própria. Os objetivos são independentes: pares e ligações de orquestração não atravessam runs.
- A barra do Team tem um seletor de run e "New run…". Uma run nova pede só o objetivo e nasce com um orquestrador (Lead) no topo.
- O mapa, o resumo e os menus mostram só os membros da run em tela.
- Mais de uma raiz na mesma run continua possível no modelo (um orquestrador com "No one"), mas o layout mistura as duas árvores. Para objetivos independentes, a regra é uma run por árvore.

### Bots

- O orquestrador pode ser um agente ou um bot do Lynx. O topo da árvore é livre para os dois.
- Um bot só orquestra: não escreve código, só lança e gerencia agentes. Ele nunca é membro comum (nem filho, nem revisor, nem outro papel sem filhos).
- Um bot não é orquestrado: não tem orquestrador acima dele. Mas pode receber mensagens de agentes (par). Ele não precisa estar no topo da árvore.
- Os filhos de um bot são agentes (as CLIs são os workers). Lançar workers num projeto exige a liberação `workers` do bot (ver "Bots" no AGENTS.md).
- No cartão: avatar e cor do bot no lugar do logo do agente, e provedor e modelo no lugar do nome do agente. Um bot não tem terminal: "Open terminal" vira "Open chat", um modal só com o chat do bot, e o objetivo da run se dá nele.
- O diálogo de adicionar membro escolhe entre agente e bot, e a lista de bots vem do painel Bots. Feito no mock; o bot é semeado só em `?mock=full`.

### Adicionar membro (mock)

- O "+" da barra do Team abre o diálogo: papel, agente, a quem responde (padrão: o primeiro orquestrador; "No one" deixa o membro sem tarefa), tarefa e "done when" (só com orquestrador) e o interruptor de orquestrador.
- Adicionar seleciona o membro novo e expande o pai.

### Linhas do mapa

- Linha contínua e mais grossa: orquestra. Linha pontilhada: conversa.
- As linhas contornam os cartões: filho na linha seguinte usa o vão entre linhas; mais abaixo, desce por um vão entre dois cartões; um par sem cartões no meio é reto, e com cartões no meio passa por cima deles.
- Ainda sem pistas separadas: linhas de pais diferentes no mesmo vão podem se sobrepor num trecho.

### Mock atual

- Run "Auth refactor":
  - Lead: orquestrador, sem tarefa, no topo.
  - Builder: orquestrador, com a tarefa "login form". Login-tests sob ele, com a tarefa "login tests".
  - Researcher e Tester: filhos do Lead, com tarefa.
  - Reviewer: sem orquestrador, sem tarefa, na linha do Builder, em par com ele.
- Run "Docs rewrite": Lead (orquestrador) e Writer, com a tarefa "setup chapter".

### Decidido

- O par é mútuo: os dois lados trocam mensagens.
- Um membro sem orquestrador só responde; não inicia conversa. Ainda não há mensagens reais no POC, então a regra fica registrada e não é aplicada.
- Apagar um bot remove em cascata o membro do bot, tudo abaixo dele e os pares ligados a eles, em todas as runs. Um bot apagado não volta para uma equipe, nem como líder de run nova.
- Abrir o chat de um bot é um modal igual ao do terminal, com só o chat: a thread mais recente do bot (ou a primeira, se não houver), sem lista de threads.
- O contraste dos cartões esmaecidos ficou em `opacity-80`, que passa no axe.

### Em aberto

- Além de tarefa e critério de pronto, o orquestrador daria o veredito de pronto e poderia trocar o papel e parar o filho. Proposto, ainda não confirmado.
- A conversa é um canal aberto: não há limite de mensagens nem contexto modelado.
- Duas raízes na mesma run: layout em blocos lado a lado, se for preciso.
- Backend do Team em Rust (`src-tauri/src/team/`): regras, SQLite e comandos feitos e testados (`rust:test`), e o frontend lê e grava por eles (`team-backend.ts`). Falta rodar contra o app real (não foi aberto com `tauri dev`).
- A aba Team aparece no app real. A expansão de cada run fica salva no navegador (`lynx.team.expanded`), mas não no banco.
- Os cartões de bot não foram vistos de perto na tela; só a captura da run Docs.
- Checagens do Team no e2e passam nas últimas rodadas. Falhas que sobram variam de rodada para rodada e não são do Team (tab bar, rodapé de planos, confirmação ao fechar aba, toolbar do Browser).
- Não verificado: arrastar um cartão com filhos para mudar de pai, e a seção "Answers to" do painel flutuante.
- Recusas do backend chegam à tela só como toast genérico; não há mensagem específica por regra.
- Linhas órfãs de bots apagados antes da cascata ficam na equipe até uma nova ação.
