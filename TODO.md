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

### Equipe por worktree

- Não existe run. A equipe é os agentes da worktree selecionada na sidebar: o mesmo conjunto de sessões que a aba de terminal mostra. Terminal e Team são duas visões da mesma coisa.
- O objetivo da run deixou de existir. O orquestrador do topo não tem tarefa.
- Uma worktree pode ter várias árvores (várias raízes orquestradoras). Elas ficam lado a lado no mapa, cada uma no seu bloco, sem se misturar.
- Pares e ligações de orquestração ficam dentro da worktree. Um bot entra uma vez em cada worktree.
- Um agente aberto pela aba de terminal vira um membro solto da equipe da worktree dele (sem orquestrador, sem tarefa). Um agente adicionado pelo Team abre a CLI na worktree selecionada.
- O status do cartão vem da sessão: trabalhando, pediu atenção, terminou (ou o processo acabou) ou ocioso.
- Fechar a sessão pela aba ou pelo Team tira o agente da equipe.
- Ao reabrir o app, ou se a sessão se perder, o cartão fica. "Resume" reabre o agente com a mesma conversa e liga a sessão nova ao membro; sem conversa para retomar, "Start again" abre uma CLI nova.

### Bots

- O orquestrador pode ser um agente ou um bot do Lynx. O topo da árvore é livre para os dois.
- Um bot só orquestra: não escreve código, só lança e gerencia agentes. Ele nunca é membro comum (nem filho, nem revisor, nem outro papel sem filhos).
- Um bot não é orquestrado: não tem orquestrador acima dele. Mas pode receber mensagens de agentes (par). Ele não precisa estar no topo da árvore.
- Os filhos de um bot são agentes (as CLIs são os workers). Lançar workers num projeto exige a liberação `workers` do bot (ver "Bots" no AGENTS.md).
- No cartão: avatar e cor do bot no lugar do logo do agente, e provedor e modelo no lugar do nome do agente. Um bot não tem terminal: "Open terminal" vira "Open chat", um modal só com o chat do bot, e o objetivo se dá nele.
- O diálogo de adicionar membro escolhe entre agente e bot, e a lista de bots vem do painel Bots. Feito no mock; o bot é semeado só em `?mock=full`.

### Adicionar membro

- O "+" da barra do Team abre o diálogo: tipo (agente ou bot), papel, agente e o interruptor de orquestrador. Sem tarefa e sem "a quem responde": a tarefa é do orquestrador, e quem liga o agente a um orquestrador usa o menu do cartão ou a linha de baixo do orquestrador.
- O agente abre a CLI na worktree selecionada, e o membro guarda a sessão.
- Adicionar seleciona o membro novo.

### Linhas do mapa

- Linha contínua e mais grossa: orquestra. Linha pontilhada: conversa.
- As linhas contornam os cartões: filho na linha seguinte usa o vão entre linhas; mais abaixo, desce por um vão entre dois cartões; um par sem cartões no meio é reto, e com cartões no meio passa por cima deles.
- Ainda sem pistas separadas: linhas de pais diferentes no mesmo vão podem se sobrepor num trecho.

### Mock atual

- Worktree feat/auth:
  - Lead: orquestrador, sem tarefa, no topo.
  - Builder: orquestrador, com a tarefa "login form". Login-tests sob ele, com a tarefa "login tests" e uma sessão encerrada para o "Resume".
  - Researcher e Tester: filhos do Lead, com tarefa.
  - Reviewer: sem orquestrador, sem tarefa, na linha do Builder, em par com ele.
  - A sessão do Claude do mock é adotada como membro solto.
- Worktree docs/readme: um bot orquestrador e o Writer, com a tarefa "setup chapter".

### Decidido

- O par é mútuo: os dois lados trocam mensagens.
- Um membro sem orquestrador só responde; não inicia conversa. Ainda não há mensagens reais no POC, então a regra fica registrada e não é aplicada.
- Apagar um bot remove em cascata o membro do bot, tudo abaixo dele e os pares ligados a eles, em todas as worktrees. Um bot apagado não volta para uma equipe.
- Abrir o chat de um bot é um modal igual ao do terminal, com só o chat: a thread mais recente do bot (ou a primeira, se não houver), sem lista de threads.
- O contraste dos cartões esmaecidos ficou em `opacity-80`, que passa no axe.
- Run deixa de existir; a equipe é a worktree, com várias árvores lado a lado.
- Agente aberto pela aba vira membro solto. Fechar a sessão tira o agente. Ao reabrir, "Resume".

### Em aberto

- Além de tarefa e critério de pronto, o orquestrador daria o veredito de pronto e poderia trocar o papel e parar o filho. Proposto, ainda não confirmado.
- Mensagem entre pares: o formulário do sheet escreve no terminal do par, com o prefixo do remetente. A mensagem não fica guardada no time, e não há histórico nem limite de mensagens.
- Backend do Team em Rust (`src-tauri/src/team/`): regras, SQLite e comandos feitos e testados (`rust:test`), e o frontend lê e grava por eles (`team-backend.ts`). Falta rodar contra o app real (não foi aberto com `tauri dev`).
- A aba Team aparece no app real.
- Checagens do Team passam no e2e completo (580 PASS, 0 FAIL na última rodada).
- Checagens do Team no e2e passam nas últimas rodadas. Falhas que sobram variam de rodada para rodada e não são do Team (tab bar, rodapé de planos, confirmação ao fechar aba, toolbar do Browser).
- Não verificado: arrastar um cartão com filhos para mudar de pai, e a seção "Answers to" do painel flutuante.
- Processo do agente que termina sozinho: a aba fica "Ended" e o cartão continua até a aba ser fechada. Confirmar se é isso.
- App real testado sob Xvfb, com dados isolados: adicionar agente, fechar aba, reinício com agente vivo e "Start again" passaram com um shim de CLI. O Claude real parou no prompt de confiança da pasta de teste; o status com o Claude de verdade ainda não foi visto.
- "Resume" depende dos hooks do agente (eles dão o id da conversa). Com os hooks desligados, o cartão só oferece "Start again". Ligar os hooks escreve em `~/.claude/settings.json`.
- O `claude --resume` pode deixar a conversa fora da lista de retomáveis se a sessão nova morrer antes de imprimir o id (mesmo trade-off da sidebar).
- Arrastar entre blocos de árvores não foi testado.
- O mapa usa zoom mínimo de 0,2 para caber em janelas estreitas com muitos cartões. Com 0,4, oito cartões em 950 px cortavam o Builder.
- Build de debug usa uma pasta por branch (`~/.lynx-dev/<branch>` e `dev/<branch>/app.db`). O localStorage do WebView continua compartilhado com o instalado.
