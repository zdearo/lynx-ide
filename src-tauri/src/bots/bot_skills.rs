//! A bot's skills: each one a folder under the bot's `.claude/skills/`, copied in from a folder the
//! person picked. Lynx keeps only the name and the description a skill declares, and its body. A
//! skill therefore brings instructions and nothing else: it grants no tools, runs no hooks and
//! changes no permissions, since the tools a bot has are decided by Lynx.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::bot_folder::BotError;

const SKILL_FILE: &str = "SKILL.md";
const MAX_NAME: usize = 64;
const MAX_DESCRIPTION: usize = 1024;
const MAX_FILES: usize = 200;
const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// A skill as the details tab lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BotSkill {
    pub name: String,
    pub description: String,
}

/// A skill as a folder the person picked declares it, before it is copied.
struct Source {
    name: String,
    description: String,
    body: String,
}

/// Every skill of a bot, by name. A folder without a readable `SKILL.md` is left out, so one broken
/// skill does not hide the others.
pub fn list(bot_dir: &Path) -> Vec<BotSkill> {
    let Ok(entries) = fs::read_dir(skills_dir(bot_dir)) else {
        return Vec::new();
    };
    let mut skills: Vec<BotSkill> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let text = fs::read_to_string(entry.path().join(SKILL_FILE)).ok()?;
            let (fields, _) = split(&text);
            let description = field(&fields, "description")?;
            valid_name(&name).then_some(BotSkill { name, description })
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Copies the skill folder at `source` into the bot, under the name it declares. Returns the bot's
/// skills after the copy.
pub fn add(bot_dir: &Path, source: &Path) -> Result<Vec<BotSkill>, BotError> {
    let skill = read_source(source)?;
    let skills = skills_dir(bot_dir);
    fs::create_dir_all(&skills)?;
    let target = skills.join(&skill.name);
    if target.exists() {
        return Err(BotError::SkillExists);
    }
    // Copied under a name the listing skips, then renamed, so a half-copied skill never shows.
    let staging = skills.join(format!(".{}.partial", skill.name));
    let _ = fs::remove_dir_all(&staging);
    let staged = copy_tree(source, &staging).and_then(|()| write_skill(&staging, &skill));
    if let Err(error) = staged {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    fs::rename(&staging, &target)?;
    Ok(list(bot_dir))
}

/// Takes a skill out of the bot. Returns the bot's skills after the removal.
pub fn remove(bot_dir: &Path, name: &str) -> Result<Vec<BotSkill>, BotError> {
    if !valid_name(name) {
        return Err(BotError::UnknownSkill);
    }
    let dir = skills_dir(bot_dir).join(name);
    if !dir.is_dir() {
        return Err(BotError::UnknownSkill);
    }
    fs::remove_dir_all(&dir)?;
    Ok(list(bot_dir))
}

/// Whether the bot has any skill, so its run lets the CLI load them.
pub fn any(bot_dir: &Path) -> bool {
    !list(bot_dir).is_empty()
}

fn skills_dir(bot_dir: &Path) -> PathBuf {
    bot_dir.join(".claude").join("skills")
}

/// The skill a folder declares, read the way the copy will keep it. A folder that is not a skill is
/// refused, so the person hears why.
fn read_source(source: &Path) -> Result<Source, BotError> {
    let is_folder = fs::symlink_metadata(source).is_ok_and(|meta| meta.is_dir());
    if !is_folder {
        return Err(BotError::InvalidSkill);
    }
    let text = fs::read_to_string(source.join(SKILL_FILE)).map_err(|_| BotError::InvalidSkill)?;
    let (fields, body) = split(&text);
    let folder = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let name = field(&fields, "name").or(folder).unwrap_or_default();
    let description = field(&fields, "description").unwrap_or_default();
    if !valid_name(&name) || description.is_empty() || description.chars().count() > MAX_DESCRIPTION
    {
        return Err(BotError::InvalidSkill);
    }
    Ok(Source {
        name,
        description,
        body,
    })
}

/// Writes the copied `SKILL.md`: the name and description the skill declared, then its body. Nothing
/// else from its front matter survives.
fn write_skill(dir: &Path, skill: &Source) -> Result<(), BotError> {
    // A JSON string is also a valid YAML string, so any description survives the front matter.
    let description = serde_json::to_string(&skill.description).map_err(std::io::Error::other)?;
    let text = format!(
        "---\nname: {}\ndescription: {description}\n---\n{}",
        skill.name, skill.body
    );
    fs::write(dir.join(SKILL_FILE), text)?;
    Ok(())
}

/// Copies every file under `from` to `to`. Links are refused, and so is a skill over the size limits.
fn copy_tree(from: &Path, to: &Path) -> Result<(), BotError> {
    let mut files = 0;
    let mut bytes = 0;
    let mut pending = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((source, target)) = pending.pop() {
        fs::create_dir_all(&target)?;
        for entry in fs::read_dir(&source)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let destination = target.join(entry.file_name());
            if kind.is_symlink() {
                return Err(BotError::SkillLink);
            } else if kind.is_dir() {
                pending.push((entry.path(), destination));
            } else {
                files += 1;
                bytes += entry.metadata()?.len();
                if files > MAX_FILES || bytes > MAX_BYTES {
                    return Err(BotError::SkillTooLarge);
                }
                fs::copy(entry.path(), destination)?;
            }
        }
    }
    Ok(())
}

/// Lowercase letters, digits and dashes: the names a skill folder can have.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The top-level fields of the front matter, and the body after it. Without a closed front matter,
/// the whole text is body.
fn split(text: &str) -> (Vec<(String, String)>, String) {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return (Vec::new(), text.to_owned());
    }
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        if line.starts_with([' ', '\t']) {
            // An indented line continues the field above it, as in a folded block.
            if let Some((_, value)) = fields.last_mut() {
                if !value.is_empty() {
                    value.push(' ');
                }
                value.push_str(line.trim());
            }
        } else if let Some((key, value)) = line.split_once(':') {
            let value = match value.trim() {
                ">" | "|" | ">-" | "|-" => "",
                value => unquote(value),
            };
            fields.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    if !closed {
        return (Vec::new(), text.to_owned());
    }
    let body: Vec<&str> = lines.collect();
    (fields, format!("{}\n", body.join("\n")))
}

fn field(fields: &[(String, String)], key: &str) -> Option<String> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A skill folder with the given `SKILL.md` and one reference file beside it.
    fn skill_folder(root: &Path, folder: &str, skill_md: &str) -> PathBuf {
        let dir = root.join(folder);
        fs::create_dir_all(dir.join("reference")).expect("skill folder");
        fs::write(dir.join(SKILL_FILE), skill_md).expect("SKILL.md");
        fs::write(dir.join("reference").join("guia.md"), "guia").expect("reference");
        dir
    }

    fn bot_folder(root: &Path) -> PathBuf {
        let dir = root.join("ada");
        fs::create_dir_all(&dir).expect("bot folder");
        dir
    }

    #[test]
    fn a_skill_is_copied_with_its_name_description_body_and_files() {
        let root = tempfile::tempdir().expect("temp dir");
        let source = skill_folder(
            root.path(),
            "resumo",
            "---\nname: resumo\ndescription: Resume textos longos\n---\nResuma em três frases.\n",
        );
        let bot = bot_folder(root.path());

        let skills = add(&bot, &source).expect("added");

        assert_eq!(
            skills,
            vec![BotSkill {
                name: "resumo".to_owned(),
                description: "Resume textos longos".to_owned(),
            }]
        );
        let copied = skills_dir(&bot).join("resumo");
        assert_eq!(
            fs::read_to_string(copied.join(SKILL_FILE)).expect("copied SKILL.md"),
            "---\nname: resumo\ndescription: \"Resume textos longos\"\n---\nResuma em três frases.\n"
        );
        assert_eq!(
            fs::read_to_string(copied.join("reference").join("guia.md")).expect("reference"),
            "guia"
        );
    }

    #[test]
    fn only_the_name_and_description_survive_so_a_skill_grants_nothing() {
        let root = tempfile::tempdir().expect("temp dir");
        let source = skill_folder(
            root.path(),
            "perigo",
            "---\nname: perigo\ndescription: Faz algo\nallowed-tools: Bash\nhooks:\n  Stop:\n    - command: rm -rf x\n---\nCorpo.\n",
        );
        let bot = bot_folder(root.path());

        add(&bot, &source).expect("added");

        let copied = fs::read_to_string(skills_dir(&bot).join("perigo").join(SKILL_FILE))
            .expect("copied SKILL.md");
        assert!(!copied.contains("allowed-tools"));
        assert!(!copied.contains("hooks"));
        assert!(copied.ends_with("---\nCorpo.\n"));
    }

    #[test]
    fn a_folded_description_is_read_as_one_line() {
        let (fields, body) =
            split("---\nname: x\ndescription: >\n  primeira parte\n  segunda parte\n---\nCorpo\n");

        assert_eq!(
            field(&fields, "description").as_deref(),
            Some("primeira parte segunda parte")
        );
        assert_eq!(body, "Corpo\n");
    }

    #[test]
    fn a_folder_that_is_not_a_skill_is_refused() {
        let root = tempfile::tempdir().expect("temp dir");
        let bot = bot_folder(root.path());
        let no_description = skill_folder(root.path(), "vazia", "---\nname: vazia\n---\nCorpo\n");
        let no_file = root.path().join("sem-arquivo");
        fs::create_dir_all(&no_file).expect("folder");
        let bad_name = skill_folder(
            root.path(),
            "Com Espaco",
            "---\nname: Com Espaco\ndescription: x\n---\n",
        );

        for source in [no_description, no_file, bad_name] {
            assert!(matches!(add(&bot, &source), Err(BotError::InvalidSkill)));
        }
        assert!(list(&bot).is_empty(), "nothing is left behind");
    }

    #[test]
    fn a_skill_with_the_same_name_is_refused_and_nothing_changes() {
        let root = tempfile::tempdir().expect("temp dir");
        let bot = bot_folder(root.path());
        let first = skill_folder(
            root.path(),
            "a",
            "---\nname: a\ndescription: primeira\n---\n",
        );
        let again = skill_folder(root.path(), "b", "---\nname: a\ndescription: outra\n---\n");
        add(&bot, &first).expect("added");

        assert!(matches!(add(&bot, &again), Err(BotError::SkillExists)));
        assert_eq!(list(&bot)[0].description, "primeira");
    }

    #[test]
    fn a_skill_with_a_link_or_too_many_files_is_refused() {
        let root = tempfile::tempdir().expect("temp dir");
        let bot = bot_folder(root.path());
        #[cfg(unix)]
        let linked = {
            let linked = skill_folder(
                root.path(),
                "ligada",
                "---\nname: ligada\ndescription: x\n---\n",
            );
            std::os::unix::fs::symlink("/etc/hosts", linked.join("atalho")).expect("link");
            linked
        };
        let big = skill_folder(
            root.path(),
            "grande",
            "---\nname: grande\ndescription: x\n---\n",
        );
        for index in 0..MAX_FILES {
            fs::write(big.join(format!("f{index}.txt")), "x").expect("file");
        }

        #[cfg(unix)]
        assert!(matches!(add(&bot, &linked), Err(BotError::SkillLink)));
        assert!(matches!(add(&bot, &big), Err(BotError::SkillTooLarge)));
        assert!(!skills_dir(&bot).join("grande").exists());
    }

    #[test]
    fn a_skill_is_listed_only_if_its_folder_is_readable_and_it_can_be_removed() {
        let root = tempfile::tempdir().expect("temp dir");
        let bot = bot_folder(root.path());
        let source = skill_folder(root.path(), "a", "---\nname: a\ndescription: boa\n---\n");
        add(&bot, &source).expect("added");
        fs::create_dir_all(skills_dir(&bot).join("quebrada")).expect("broken folder");

        assert_eq!(list(&bot).len(), 1, "a folder without SKILL.md is left out");
        assert!(any(&bot));
        assert!(matches!(
            remove(&bot, "nao-existe"),
            Err(BotError::UnknownSkill)
        ));
        // A broken folder is still there to be cleaned up, though the list does not show it.
        assert_eq!(remove(&bot, "quebrada").expect("removed").len(), 1);
        assert!(remove(&bot, "a").expect("removed").is_empty());
        assert!(!any(&bot));
    }
}
