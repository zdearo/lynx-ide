//! The tools a bot calls through Lynx's MCP server. A bot can list the registered repositories and
//! ask the person for permission: to read a project, or to run commands that match patterns. It
//! cannot approve its own request: the person does that in the chat.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::agents::hook_hub::ToolHandler;

use super::bot_access::{AccessRequest, AccessState, RegisteredRepos, RequestKind, find_repo};
use super::bot_bridge::parse_session;
use super::bot_chat::{BotChat, RequestOutcome};
use super::bot_conversation::ChatMessage;
use super::bot_processes::BotProcesses;
use super::bot_thread;
use super::command_permissions;

/// The prefix of every tool a bot has, which the hub uses to send the call to the bots.
pub const PREFIX: &str = "bot_";
pub const LIST: &str = "bot_list_workspaces";
pub const REQUEST: &str = "bot_request";
pub const START_COMMAND: &str = "bot_start_command";
pub const COMMANDS_STATUS: &str = "bot_commands_status";
pub const STOP_COMMAND: &str = "bot_stop_command";
pub const MEMORY: &str = "bot_memory_update";
pub const MEMORY_TOPIC: &str = "bot_memory_topic";

/// What a bot's skill says about its own tools: how they are called, and what a refusal means.
pub const INSTRUCTIONS: &str = "Chame as ferramentas bot_* e lynx_* pelo Bash, com o comando desta skill. bot_request pede uma permissão à pessoa: ela aprova no chat e você retoma sozinho depois; os tipos são project (ler um projeto), workers (lançar agentes num projeto que você já lê) e commands (padrões de comando). Quando uma ferramenta recusar, o erro diz o motivo: repita à pessoa esse motivo, sem trocar por outro.";
/// The most items one request may carry.
const MAX_ITEMS: usize = 20;
/// The longest reason a bot may give, in characters.
const MAX_REASON: usize = 500;

/// Shows a message that was just saved, so the chat has it without a reload.
pub type Announce = Arc<dyn Fn(&str, &ChatMessage) + Send + Sync>;

/// What a request turns into: something to ask the person, or an answer that needs no asking.
enum Prepared {
    Ask(AccessRequest, String),
    Answer(String),
}

pub struct BotTools {
    chat: Arc<BotChat>,
    repos: Arc<dyn RegisteredRepos>,
    announce: Announce,
    processes: Arc<BotProcesses>,
}

impl BotTools {
    pub fn new(
        chat: Arc<BotChat>,
        repos: Arc<dyn RegisteredRepos>,
        announce: Announce,
        processes: Arc<BotProcesses>,
    ) -> Self {
        Self {
            chat,
            repos,
            announce,
            processes,
        }
    }

    fn list(&self, bot: &str) -> Result<String, String> {
        let grants = self.chat.grants(bot).map_err(|error| error.to_string())?;
        let repos = self.repos.repos();
        if repos.is_empty() {
            return Ok("Nenhum projeto está registrado no Lynx.".to_owned());
        }
        let lines: Vec<String> = repos
            .iter()
            .map(|repo| {
                let granted = if grants.iter().any(|grant| grant.id == repo.id) {
                    " — você já pode ler"
                } else {
                    ""
                };
                format!("- {} (id: {}){granted}", repo.name, repo.id)
            })
            .collect();
        Ok(format!("Projetos registrados:\n{}", lines.join("\n")))
    }

    fn request(&self, bot: &str, label: &str, arguments: &Value) -> Result<String, String> {
        let kind = match arguments.get("kind").and_then(Value::as_str) {
            Some("project") => RequestKind::Project,
            Some("commands") => RequestKind::Commands,
            Some("workers") => RequestKind::Workers,
            _ => return Err("Informe kind: \"project\" ou \"commands\".".to_owned()),
        };
        let items: Vec<String> = arguments
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if items.is_empty() || items.len() > MAX_ITEMS {
            return Err(format!("Informe de 1 a {MAX_ITEMS} itens em items."));
        }
        let reason = arguments
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .ok_or("Informe o motivo com o campo reason.")?;
        let reason: String = reason.chars().take(MAX_REASON).collect();
        let prepared = match kind {
            RequestKind::Project => self.project_request(bot, &items)?,
            RequestKind::Commands => self.commands_request(bot, &items)?,
            RequestKind::Workers => self.workers_request(bot, &items)?,
        };
        let (request, shown) = match prepared {
            Prepared::Ask(request, shown) => (request, shown),
            Prepared::Answer(text) => return Ok(text),
        };
        match self
            .chat
            .request_access(bot, label, request, &reason)
            .map_err(|error| error.to_string())?
        {
            RequestOutcome::Asked(message, thread) => {
                (self.announce)(&bot_thread::key_of(bot, &thread), &message);
                Ok(format!(
                    "Pedi à pessoa: {shown}. Ela aprova no chat, e você retoma a tarefa depois da aprovação."
                ))
            }
            RequestOutcome::AlreadyAsked => Ok(format!(
                "O pedido ({shown}) já está esperando a aprovação da pessoa."
            )),
        }
    }

    fn memory(&self, bot: &str, arguments: &Value) -> Result<String, String> {
        let text = |field: &str| {
            arguments
                .get(field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
        };
        let (add, remove) = (text("add"), text("remove"));
        if add.is_none() && remove.is_none() {
            return Err(
                "Informe add (uma nota para guardar) ou remove (um trecho da nota a apagar)."
                    .to_owned(),
            );
        }
        let mut said = Vec::new();
        if let Some(snippet) = remove {
            let removed = self
                .chat
                .memory_remove(bot, snippet)
                .map_err(|error| error.to_string())?;
            said.push(format!(
                "Apaguei {removed} nota(s) que citavam \"{snippet}\"."
            ));
        }
        if let Some(entry) = add {
            let added = self
                .chat
                .memory_add(bot, entry)
                .map_err(|error| error.to_string())?;
            said.push(if added.moved == 0 {
                "Anotei.".to_owned()
            } else {
                format!("Anotei. A memória estava cheia, então {} nota(s) antiga(s) foram para o arquivo.", added.moved)
            });
        }
        Ok(said.join(" "))
    }

    fn memory_topic(&self, bot: &str, arguments: &Value) -> Result<String, String> {
        let text = |field: &str| {
            arguments
                .get(field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
        };
        let topic = text("topic").ok_or("Informe o tópico com o campo topic.")?;
        let failed = |error: crate::bots::bot_folder::BotError| error.to_string();
        if let Some(entry) = text("add") {
            self.chat
                .topic_add(bot, topic, text("title"), text("aliases"), entry)
                .map_err(failed)?;
            return Ok(format!("Anotei no tópico {topic}."));
        }
        if let Some(whole) = text("write") {
            self.chat.topic_write(bot, topic, whole).map_err(failed)?;
            return Ok(format!("Reescrevi o tópico {topic}."));
        }
        self.chat.topic_read(bot, topic).map_err(failed)
    }

    fn start_command(&self, bot: &str, label: &str, arguments: &Value) -> Result<String, String> {
        let command = arguments
            .get("command")
            .and_then(Value::as_str)
            .ok_or("Informe o comando com o campo command.")?;
        let session = self.processes.start(bot, label, command)?;
        Ok(format!(
            "Iniciei \"{}\". Id: {session}. Ele continua rodando depois da sua resposta. Use {COMMANDS_STATUS} para ver a saída e {STOP_COMMAND} para parar.",
            command.trim()
        ))
    }

    fn commands_status(&self, bot: &str) -> String {
        let commands = self.processes.status(bot);
        if commands.is_empty() {
            return "Você não iniciou nenhum comando.".to_owned();
        }
        commands
            .iter()
            .map(|one| {
                let state = if one.running { "rodando" } else { "terminou" };
                format!(
                    "- {} (id: {}, {state})\n{}",
                    one.command,
                    one.session,
                    if one.output.is_empty() {
                        "(sem saída)"
                    } else {
                        &one.output
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn stop_command(&self, bot: &str, arguments: &Value) -> Result<String, String> {
        let session = arguments
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Informe o id do comando com o campo id.")?;
        self.processes
            .stop(bot, session)
            .map_err(|error| error.to_string())?;
        Ok("Parei o comando.".to_owned())
    }

    /// A request to read one registered project, and how to name it to the bot.
    fn project_request(&self, bot: &str, items: &[String]) -> Result<Prepared, String> {
        let [query] = items else {
            return Err("Para project, informe um só projeto em items (nome ou id).".to_owned());
        };
        let repos = self.repos.repos();
        let repo = find_repo(&repos, query).ok_or_else(|| {
            format!("Não há projeto registrado com o nome ou id \"{query}\". Use {LIST} para ver a lista.")
        })?;
        let grants = self.chat.grants(bot).map_err(|error| error.to_string())?;
        if grants.iter().any(|grant| grant.id == repo.id) {
            return Ok(Prepared::Answer(format!("Você já pode ler {}.", repo.name)));
        }
        let request = AccessRequest {
            kind: RequestKind::Project,
            items: vec![repo.name.clone()],
            repo_id: Some(repo.id.clone()),
            state: AccessState::Pending,
        };
        Ok(Prepared::Ask(request, format!("ler {}", repo.name)))
    }

    /// A request to start agents in one registered project the bot already reads. Its workers run
    /// without asking for permission, so this is the only thing between the bot and them.
    fn workers_request(&self, bot: &str, items: &[String]) -> Result<Prepared, String> {
        let [query] = items else {
            return Err("Para workers, informe um só projeto em items (nome ou id).".to_owned());
        };
        let repos = self.repos.repos();
        let repo = find_repo(&repos, query).ok_or_else(|| {
            format!("Não há projeto registrado com o nome ou id \"{query}\". Use {LIST} para ver a lista.")
        })?;
        let reads = self.chat.grants(bot).map_err(|error| error.to_string())?;
        if !reads.iter().any(|grant| grant.id == repo.id) {
            return Err(format!(
                "Você ainda não lê {}. Peça primeiro com kind \"project\".",
                repo.name
            ));
        }
        let granted = self
            .chat
            .worker_grants(bot)
            .map_err(|error| error.to_string())?;
        if granted.iter().any(|grant| grant.id == repo.id) {
            return Ok(Prepared::Answer(format!(
                "Você já pode lançar agentes em {}.",
                repo.name
            )));
        }
        let request = AccessRequest {
            kind: RequestKind::Workers,
            items: vec![repo.name.clone()],
            repo_id: Some(repo.id.clone()),
            state: AccessState::Pending,
        };
        Ok(Prepared::Ask(
            request,
            format!("lançar agentes em {}", repo.name),
        ))
    }

    /// A request to run commands. A pattern Lynx refuses fails the whole request, with the reason;
    /// patterns the bot has already are left out, and when none is left there is nothing to ask.
    fn commands_request(&self, bot: &str, items: &[String]) -> Result<Prepared, String> {
        let allowed = self.chat.commands(bot).map_err(|error| error.to_string())?;
        let mut wanted: Vec<String> = Vec::new();
        for item in items {
            let pattern = command_permissions::check(item).map_err(|error| error.to_string())?;
            if !allowed.contains(&pattern) && !wanted.contains(&pattern) {
                wanted.push(pattern);
            }
        }
        if wanted.is_empty() {
            return Ok(Prepared::Answer(
                "Você já pode rodar esses comandos.".to_owned(),
            ));
        }
        let shown = format!("rodar {}", wanted.join(", "));
        let request = AccessRequest {
            kind: RequestKind::Commands,
            items: wanted,
            repo_id: None,
            state: AccessState::Pending,
        };
        Ok(Prepared::Ask(request, shown))
    }
}

impl ToolHandler for BotTools {
    fn call(&self, session_id: &str, _agent: &str, tool: &str, arguments: Value) -> Value {
        let Some((bot, label)) = parse_session(session_id) else {
            return result("Só um bot pode usar esta ferramenta.", true);
        };
        let outcome = match tool {
            LIST => self.list(bot),
            REQUEST => self.request(bot, label, &arguments),
            START_COMMAND => self.start_command(bot, label, &arguments),
            COMMANDS_STATUS => Ok(self.commands_status(bot)),
            STOP_COMMAND => self.stop_command(bot, &arguments),
            MEMORY => self.memory(bot, &arguments),
            MEMORY_TOPIC => self.memory_topic(bot, &arguments),
            other => Err(format!("A ferramenta {other} não existe para bots.")),
        };
        match outcome {
            Ok(text) => result(&text, false),
            Err(text) => result(&text, true),
        }
    }
}

/// The tool list a bot's MCP server offers: its own tools, and the orchestration tools of a coordinator.
pub fn definitions() -> Vec<Value> {
    let mut all = own_definitions();
    all.extend(coordinator_definitions());
    all
}

/// The name of every tool a bot may call. Only the tests list them now: a bot's shell reaches them by the CLI.
#[cfg(test)]
pub fn names() -> Vec<String> {
    definitions()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect()
}

/// The orchestration tools a bot coordinates with: an agent's, without `lynx_report` (only a worker
/// calls it), and with the worktree choice a bot makes itself.
fn coordinator_definitions() -> Vec<Value> {
    crate::orchestration::tools::definitions()
        .into_iter()
        .filter_map(|tool| match tool["name"].as_str() {
            Some("lynx_report") => None,
            Some("lynx_worker_start") => Some(worker_start()),
            _ => Some(tool),
        })
        .collect()
}

fn worker_start() -> Value {
    json!({
        "name": "lynx_worker_start",
        "description": "Starts an agent for one of your members whose task is pending and whose waits are done, with the agent and model the user chose. Choose the worktree yourself: \"new\" for work that stands apart from what is already running, which is the usual case; \"current\" to work in the team's own worktree, next to the files of the other members.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "memberId": { "type": "string" },
                "agent": { "type": "string", "enum": crate::orchestration::tools::AGENTS },
                "worktree": { "type": "string", "enum": ["new", "current"] },
                "model": { "type": "string", "description": "A model name the agent accepts, if the user named one. Not for OpenCode." },
            },
            "required": ["memberId", "agent", "worktree"],
        },
        "annotations": { "readOnlyHint": false, "openWorldHint": false },
    })
}

fn own_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": LIST,
            "description": "Lista os projetos registrados no Lynx, com o id de cada um e se você já pode lê-los.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": MEMORY,
            "description": "Guarda ou apaga notas da sua memória, que volta nas próximas conversas. Use add para guardar um fato duradouro que a pessoa pediu para você lembrar, ou que ela claramente quer que você saiba (preferências, nomes, decisões). Use remove, com um trecho da nota, para apagar uma nota desatualizada. Nunca guarde senhas, chaves ou tokens: eles são apagados.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "add": { "type": "string", "description": "A nota a guardar, curta e autoexplicativa." },
                    "remove": { "type": "string", "description": "Um trecho (3 caracteres ou mais) das notas a apagar." },
                },
            },
        }),
        json!({
            "name": MEMORY_TOPIC,
            "description": "Lê ou escreve um tópico da sua memória: notas longas sobre um assunto (uma viagem, um projeto, uma pessoa), que não carregam sozinhas. Sem add nem write, devolve o texto do tópico. Com add, acrescenta uma nota datada (cria o tópico se for novo). Com write, substitui o tópico inteiro, para corrigir. Nunca guarde senhas, chaves ou tokens: eles são apagados.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "description": "O nome do tópico: letras minúsculas, dígitos e hífen, como viagem-lisboa." },
                    "add": { "type": "string", "description": "A nota a acrescentar, curta e autoexplicativa." },
                    "write": { "type": "string", "description": "O texto novo do tópico inteiro." },
                    "title": { "type": "string", "description": "Só num tópico novo: o título." },
                    "aliases": { "type": "string", "description": "Só num tópico novo: outras palavras para o assunto, separadas por vírgula." },
                },
                "required": ["topic"],
            },
        }),
        json!({
            "name": START_COMMAND,
            "description": "Inicia um comando que continua rodando depois da sua resposta, como um servidor de desenvolvimento. O comando precisa casar com um padrão que a pessoa liberou (peça com bot_request, kind \"commands\") e ser só o programa e os argumentos: sem ;, &&, |, $, crase, redirecionamento ou quebra de linha. Para comandos que terminam logo, use a ferramenta Bash.",
            "inputSchema": {
                "type": "object",
                "properties": { "command": { "type": "string", "description": "O programa e os argumentos." } },
                "required": ["command"],
            },
        }),
        json!({
            "name": COMMANDS_STATUS,
            "description": "Lista os comandos que você iniciou com bot_start_command: se ainda rodam e o fim do que imprimiram.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": STOP_COMMAND,
            "description": "Para um comando que você iniciou, pelo id que bot_start_command ou bot_commands_status deram.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string", "description": "O id do comando." } },
                "required": ["id"],
            },
        }),
        json!({
            "name": REQUEST,
            "description": "Pede à pessoa uma permissão. A pessoa aprova no chat; depois você retoma a tarefa sozinho. Use kind \"project\" com o nome ou id de um projeto (de bot_list_workspaces) para poder lê-lo. Use kind \"workers\" com o nome de um projeto que você já lê, para poder lançar agentes (workers) nele com as ferramentas lynx_*: eles rodam sem pedir permissão a ninguém, então diga no motivo o que vão fazer. Use kind \"commands\" com padrões de comando, como \"minha-cli *\", para poder rodá-los: o padrão começa pelo programa, e o * cobre os argumentos. Peça só o que a tarefa precisa. Não existe padrão que libere qualquer comando, e shells como bash, sh e sudo são recusados.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["project", "commands", "workers"], "description": "O que você pede." },
                    "items": { "type": "array", "items": { "type": "string" }, "description": "Para project e workers: um projeto. Para commands: os padrões de comando." },
                    "reason": { "type": "string", "description": "Por que você precisa disso." },
                },
                "required": ["kind", "items", "reason"],
            },
        }),
    ]
}

fn result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use tempfile::TempDir;

    use super::*;
    use crate::bots::bot_access::{Grant, RepoRef};
    use crate::bots::bot_bridge::session_of;
    use crate::bots::bot_conversation;
    use crate::bots::bot_folder::{self, NewBot};
    use crate::bots::bot_processes::CommandHost;
    use crate::bots::claude_cli::ClaudeCli;

    /// Terminals that only exist in name: a command starts as `t1`, `t2`… and keeps running.
    #[derive(Default)]
    struct NoTerminals(Mutex<Vec<String>>);

    impl CommandHost for NoTerminals {
        fn start(&self, _cwd: &std::path::Path, command: &str) -> Result<String, String> {
            let mut started = self.0.lock().expect("lock");
            started.push(command.to_owned());
            Ok(format!("t{}", started.len()))
        }

        fn running(&self, _session: &str) -> bool {
            true
        }

        fn output(&self, _session: &str) -> String {
            "listening on 8080".to_owned()
        }

        fn stop(&self, _session: &str) {}
    }

    /// The conversation of the bot's most recent thread, where its own tools write.
    fn thread_conversation(
        bot_dir: &std::path::Path,
    ) -> Result<Vec<ChatMessage>, bot_folder::BotError> {
        let thread = bot_thread::latest(bot_dir)?;
        bot_conversation::read(&bot_thread::dir(bot_dir, &thread)?)
    }

    struct Repos(Vec<RepoRef>);

    impl RegisteredRepos for Repos {
        fn repos(&self) -> Vec<RepoRef> {
            self.0.clone()
        }

        fn folders_of(&self, _repo_id: &str) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    fn repo(id: &str, name: &str) -> RepoRef {
        RepoRef {
            id: id.to_owned(),
            name: name.to_owned(),
        }
    }

    /// A bots folder with one bot, and the tools over it. The second value is what was announced.
    fn setup(repos: Vec<RepoRef>) -> (TempDir, BotTools, String, Arc<Mutex<Vec<String>>>) {
        let root = TempDir::new().expect("temp dir");
        let bots = root.path().join("bots");
        let bot = bot_folder::create(
            &bots,
            NewBot {
                name: "Ada".to_owned(),
                color: "rose".to_owned(),
                provider: "claude".to_owned(),
                model: None,
                instructions: "Ajude.".to_owned(),
                team: None,
            },
        )
        .expect("the bot is made")
        .id;
        let chat = Arc::new(BotChat::new(
            bots.clone(),
            Arc::new(ClaudeCli::new(bots.clone(), root.path().to_path_buf())),
        ));
        let bots_dir = bots;
        let announced = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&announced);
        let announce: Announce = Arc::new(move |thread: &str, _message: &ChatMessage| {
            seen.lock().expect("lock").push(thread.to_owned());
        });
        let processes = Arc::new(BotProcesses::new(
            Arc::new(NoTerminals::default()),
            bots_dir,
        ));
        let tools = BotTools::new(chat, Arc::new(Repos(repos)), announce, processes);
        (root, tools, bot, announced)
    }

    /// Calls a tool as the bot's session, and returns whether it failed and the text it answered.
    fn call(tools: &BotTools, bot: &str, tool: &str, arguments: Value) -> (bool, String) {
        let answer = tools.call(&session_of(bot, "test"), "claude", tool, arguments);
        let text = answer["content"][0]["text"].as_str().unwrap_or_default();
        (answer["isError"].as_bool().unwrap_or(true), text.to_owned())
    }

    #[test]
    fn a_bot_lists_the_registered_repos_and_asks_to_read_one() {
        let (_root, tools, bot, announced) = setup(vec![repo("r1", "lynx")]);

        let (failed, listed) = call(&tools, &bot, LIST, json!({}));
        assert!(!failed);
        assert!(listed.contains("lynx (id: r1)"), "{listed}");

        let (failed, asked) = call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "project", "items": ["lynx"], "reason": "preciso ler o código" }),
        );
        assert!(!failed, "{asked}");
        assert!(asked.contains("Pedi à pessoa: ler lynx"), "{asked}");
        let announced = announced.lock().expect("lock");
        assert_eq!(announced.len(), 1);
        assert!(
            announced[0].starts_with(&format!("bot:{bot}:")),
            "announced in the thread the bot is answering in: {announced:?}"
        );
    }

    #[test]
    fn a_request_still_waiting_is_not_asked_again_and_an_unknown_repo_is_refused() {
        let (_root, tools, bot, announced) = setup(vec![repo("r1", "lynx")]);
        let ask = json!({ "kind": "project", "items": ["r1"], "reason": "preciso ler" });

        call(&tools, &bot, REQUEST, ask.clone());
        let (failed, again) = call(&tools, &bot, REQUEST, ask);
        assert!(!failed);
        assert!(again.contains("já está esperando"), "{again}");
        assert_eq!(announced.lock().expect("lock").len(), 1);

        let (failed, unknown) = call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "project", "items": ["outro"], "reason": "x" }),
        );
        assert!(failed);
        assert!(unknown.contains("Não há projeto"), "{unknown}");
    }

    #[test]
    fn a_repo_the_bot_already_reads_is_not_asked_for_again() {
        let (root, tools, bot, _announced) = setup(vec![repo("r1", "lynx")]);
        bot_folder::grant(
            &root.path().join("bots"),
            &bot,
            Grant {
                id: "r1".to_owned(),
                name: "lynx".to_owned(),
            },
        )
        .expect("granted");

        let (failed, answer) = call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "project", "items": ["lynx"], "reason": "preciso" }),
        );
        assert!(!failed);
        assert!(answer.contains("já pode ler"), "{answer}");
    }

    #[test]
    fn a_bot_asks_to_run_command_patterns_and_the_card_lists_them() {
        let (root, tools, bot, announced) = setup(Vec::new());

        let (failed, asked) = call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "commands", "items": ["minha-cli *", " git status "], "reason": "rodar a CLI" }),
        );
        assert!(!failed, "{asked}");
        assert!(asked.contains("rodar minha-cli *, git status"), "{asked}");
        assert_eq!(announced.lock().expect("lock").len(), 1);
        let kept = thread_conversation(&root.path().join("bots").join(&bot))
            .expect("the conversation")
            .into_iter()
            .find_map(|turn| turn.request)
            .expect("the request is kept");
        assert_eq!(kept.kind, RequestKind::Commands);
        assert_eq!(kept.items, ["minha-cli *", "git status"]);
        assert_eq!(kept.repo_id, None);

        let (failed, again) = call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "commands", "items": ["git status", "minha-cli *"], "reason": "de novo" }),
        );
        assert!(!failed);
        assert!(again.contains("já está esperando"), "{again}");
        assert_eq!(
            announced.lock().expect("lock").len(),
            1,
            "the same ask is not made twice"
        );
    }

    #[test]
    fn a_pattern_that_allows_everything_fails_the_request_and_nothing_is_asked() {
        let (_root, tools, bot, announced) = setup(Vec::new());

        for items in [
            json!(["bash *"]),
            json!(["*"]),
            json!(["minha-cli *", "sudo rm *"]),
        ] {
            let (failed, answer) = call(
                &tools,
                &bot,
                REQUEST,
                json!({ "kind": "commands", "items": items, "reason": "x" }),
            );
            assert!(failed, "{items}");
            assert!(answer.contains("not allowed"), "{answer}");
        }
        assert!(announced.lock().expect("lock").is_empty());
    }

    #[test]
    fn patterns_the_bot_already_has_are_not_asked_again() {
        let (root, tools, bot, announced) = setup(Vec::new());
        command_permissions::allow(&root.path().join("bots"), &bot, &["minha-cli *".to_owned()])
            .expect("allowed");

        let (failed, answer) = call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "commands", "items": ["minha-cli *"], "reason": "x" }),
        );
        assert!(!failed);
        assert!(answer.contains("já pode rodar"), "{answer}");
        assert!(announced.lock().expect("lock").is_empty());
    }

    #[test]
    fn a_request_needs_a_known_kind_items_and_a_reason() {
        let (_root, tools, bot, _announced) = setup(vec![repo("r1", "lynx")]);

        for arguments in [
            json!({ "items": ["lynx"], "reason": "x" }),
            json!({ "kind": "shell", "items": ["lynx"], "reason": "x" }),
            json!({ "kind": "project", "items": [], "reason": "x" }),
            json!({ "kind": "project", "items": ["lynx", "r1"], "reason": "x" }),
            json!({ "kind": "project", "items": ["lynx"] }),
        ] {
            let (failed, _) = call(&tools, &bot, REQUEST, arguments.clone());
            assert!(failed, "{arguments}");
        }
    }

    #[test]
    fn a_bot_leaves_an_allowed_command_running_and_looks_at_it_and_stops_it() {
        let (root, tools, bot, _announced) = setup(Vec::new());
        command_permissions::allow(&root.path().join("bots"), &bot, &["minha-cli *".to_owned()])
            .expect("allowed");

        let (failed, started) = call(
            &tools,
            &bot,
            START_COMMAND,
            json!({ "command": "minha-cli serve" }),
        );
        assert!(!failed, "{started}");
        assert!(started.contains("Id: t1"), "{started}");

        let (_, status) = call(&tools, &bot, COMMANDS_STATUS, json!({}));
        assert!(
            status.contains("minha-cli serve (id: t1, rodando)"),
            "{status}"
        );
        assert!(status.contains("listening on 8080"), "{status}");

        let (failed, stopped) = call(&tools, &bot, STOP_COMMAND, json!({ "id": "t1" }));
        assert!(!failed, "{stopped}");
        let (failed, _) = call(&tools, &bot, STOP_COMMAND, json!({ "id": "t9" }));
        assert!(failed, "an id the bot did not start");
    }

    #[test]
    fn a_command_the_person_did_not_allow_does_not_start_and_says_how_to_ask() {
        let (root, tools, bot, _announced) = setup(Vec::new());
        command_permissions::allow(&root.path().join("bots"), &bot, &["minha-cli *".to_owned()])
            .expect("allowed");

        for command in ["rm -rf x", "minha-cli a; rm -rf x"] {
            let (failed, answer) = call(&tools, &bot, START_COMMAND, json!({ "command": command }));
            assert!(failed, "{command}");
            assert!(!answer.is_empty());
        }
        let (_, answer) = call(
            &tools,
            &bot,
            START_COMMAND,
            json!({ "command": "rm -rf x" }),
        );
        assert!(answer.contains("bot_request"), "{answer}");
        let (_, status) = call(&tools, &bot, COMMANDS_STATUS, json!({}));
        assert!(status.contains("nenhum comando"), "{status}");
    }

    #[test]
    fn a_bot_keeps_a_note_and_forgets_it_and_cannot_keep_a_secret() {
        let (root, tools, bot, _announced) = setup(Vec::new());
        let dir = root.path().join("bots").join(&bot);

        let (failed, said) = call(
            &tools,
            &bot,
            MEMORY,
            json!({ "add": "A pessoa prefere respostas curtas. Chave sk-abcdefghijklmnopqrstuvwxyz123456" }),
        );
        assert!(!failed, "{said}");
        assert!(said.contains("Anotei"), "{said}");
        let memory = crate::bots::bot_memory::read(&dir);
        assert!(
            memory.contains("prefere respostas curtas") && !memory.contains("sk-abc"),
            "{memory}"
        );

        let (failed, said) = call(
            &tools,
            &bot,
            MEMORY,
            json!({ "remove": "respostas curtas" }),
        );
        assert!(!failed && said.contains("Apaguei 1"), "{said}");
        assert_eq!(crate::bots::bot_memory::read(&dir), "");

        let (failed, _) = call(&tools, &bot, MEMORY, json!({}));
        assert!(failed, "nothing to do");
    }

    #[test]
    fn a_bot_asks_to_start_agents_only_in_a_project_it_reads_and_not_again_once_allowed() {
        let (root, tools, bot, announced) = setup(vec![repo("r1", "lynx")]);
        let bots = root.path().join("bots");
        let ask = json!({ "kind": "workers", "items": ["lynx"], "reason": "vou revisar o PR" });

        let (failed, answer) = call(&tools, &bot, REQUEST, ask.clone());
        assert!(failed, "it does not read the project yet");
        assert!(answer.contains("não lê lynx"), "{answer}");

        bot_folder::grant(
            &bots,
            &bot,
            Grant {
                id: "r1".to_owned(),
                name: "lynx".to_owned(),
            },
        )
        .expect("granted");
        let (failed, asked) = call(&tools, &bot, REQUEST, ask.clone());
        assert!(!failed, "{asked}");
        assert!(asked.contains("lançar agentes em lynx"), "{asked}");
        assert_eq!(announced.lock().expect("lock").len(), 1);
        let kept = thread_conversation(&bots.join(&bot))
            .expect("the conversation")
            .into_iter()
            .find_map(|turn| turn.request)
            .expect("the request is kept");
        assert_eq!(kept.kind, RequestKind::Workers);
        assert_eq!(kept.repo_id.as_deref(), Some("r1"));

        bot_folder::grant_workers(
            &bots,
            &bot,
            Grant {
                id: "r1".to_owned(),
                name: "lynx".to_owned(),
            },
        )
        .expect("allowed");
        let (failed, answer) = call(&tools, &bot, REQUEST, ask);
        assert!(!failed && answer.contains("já pode lançar"), "{answer}");
    }

    #[test]
    fn a_bot_coordinates_with_the_orchestration_tools_but_does_not_report() {
        let names = names();

        assert!(names.contains(&"lynx_member_add".to_owned()));
        assert!(names.contains(&"lynx_worker_start".to_owned()));
        assert!(names.contains(&"lynx_check".to_owned()));
        assert!(!names.contains(&"lynx_report".to_owned()));
    }

    #[test]
    fn a_request_is_answered_once_and_only_by_a_bot_session() {
        let (root, tools, bot, _announced) = setup(vec![repo("r1", "lynx")]);
        call(
            &tools,
            &bot,
            REQUEST,
            json!({ "kind": "project", "items": ["r1"], "reason": "preciso ler" }),
        );
        let bots = root.path().join("bots");
        let thread = bot_thread::latest(&bots.join(&bot)).expect("the bot's thread");
        let asked = thread_conversation(&bots.join(&bot))
            .expect("the conversation")
            .into_iter()
            .find(|turn| turn.request.is_some())
            .expect("the request is kept");

        let approved = tools
            .chat
            .decide_request(&bot, &thread, &asked.id, AccessState::Approved)
            .expect("answered");
        assert_eq!(approved.state, AccessState::Approved);
        assert!(
            tools
                .chat
                .decide_request(&bot, &thread, &asked.id, AccessState::Denied)
                .is_err()
        );

        let other = tools.call("s1", "claude", LIST, json!({}));
        assert_eq!(other["isError"], json!(true));
    }
}
