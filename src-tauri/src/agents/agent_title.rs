//! What a harness's terminal title says about its state, for sessions without hooks. Each agent
//! writes its title its own way: Claude a spinner or a `✳` prefix, OpenCode an `OC |` label, OMP a
//! state marker after `π`, and Codex its name with a state word.

use super::harness::HarnessId;

/// What a title says the agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleSignal {
    Working,
    NeedsYou,
    /// The turn is over, or the agent waits for the next prompt.
    Rest,
}

/// The signal a title carries for a harness, or None when the title says nothing about its state.
pub fn signal_of(harness: HarnessId, title: &str) -> Option<TitleSignal> {
    let title = title.trim_start();
    match harness {
        HarnessId::Omp => {
            if let Some(signal) = pi_marker(title) {
                return Some(signal);
            }
        }
        HarnessId::Opencode if is_opencode_title(title) => {
            return Some(if has_spinner(title) {
                TitleSignal::Working
            } else {
                TitleSignal::Rest
            });
        }
        _ => {}
    }
    if title.to_ascii_lowercase().contains("action required") {
        return Some(TitleSignal::NeedsYou);
    }
    if harness == HarnessId::Claude {
        if title.starts_with('✳') || title.starts_with("* ") {
            return Some(TitleSignal::Rest);
        }
        if title.starts_with(". ") || has_spinner(title) {
            return Some(TitleSignal::Working);
        }
        return None;
    }
    if has_spinner(title) {
        return Some(TitleSignal::Working);
    }
    match harness {
        HarnessId::Codex => named(title, &["codex"]),
        // OpenCode and OMP titles that did not match their own form say nothing.
        HarnessId::Claude | HarnessId::Opencode | HarnessId::Omp => None,
    }
}

/// Braille patterns, which terminal UIs animate as a spinner while they work.
fn has_spinner(title: &str) -> bool {
    title
        .chars()
        .any(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
}

/// OpenCode's own title: `OC | <task>`, with an optional status glyph in front of it.
fn is_opencode_title(title: &str) -> bool {
    let rest =
        title.trim_start_matches(|ch: char| ch == '▣' || ('\u{2800}'..='\u{28ff}').contains(&ch));
    let rest = rest.trim_start();
    rest.strip_prefix("OC |")
        .is_some_and(|task| task.starts_with(char::is_whitespace) && !task.trim().is_empty())
}

/// OMP writes its state after its brand: `π : label` works, `π > label` is at rest,
/// `π ! label` needs the user. A braille frame in that place is work too.
fn pi_marker(title: &str) -> Option<TitleSignal> {
    let after_brand = ["π", "Pi", "OMP"]
        .iter()
        .find_map(|brand| title.strip_prefix(brand))?;
    let mut chars = after_brand.trim_start().chars();
    let marker = chars.next()?;
    if !after_brand.starts_with(char::is_whitespace) {
        return None;
    }
    match marker {
        ':' => Some(TitleSignal::Working),
        '>' => Some(TitleSignal::Rest),
        '!' => Some(TitleSignal::NeedsYou),
        marker if ('\u{2800}'..='\u{28ff}').contains(&marker) => Some(TitleSignal::Working),
        _ => None,
    }
}

/// The state a title gives by a state word, when it names the agent. A title without the name is
/// about something else, such as a path that ends in `ready`.
fn named(title: &str, names: &[&str]) -> Option<TitleSignal> {
    if !names.iter().any(|name| has_token(title, name)) {
        return None;
    }
    if has_token(title, "permission") || has_token(title, "waiting") {
        return Some(TitleSignal::NeedsYou);
    }
    if ["ready", "idle", "done"]
        .iter()
        .any(|word| has_token(title, word))
    {
        return Some(TitleSignal::Rest);
    }
    if ["working", "thinking", "running"]
        .iter()
        .any(|word| has_token(title, word))
    {
        return Some(TitleSignal::Working);
    }
    None
}

/// Whether `word` appears on its own, case-blind. A word glued to a path, a hyphen or another word
/// does not count, so `~/codex/ready` and `reworking` say nothing.
fn has_token(title: &str, word: &str) -> bool {
    let lower = title.to_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(word) {
        let start = from + found;
        let end = start + word.len();
        let before = lower[..start].chars().next_back();
        let after = lower[end..].chars().next();
        let glued_before = before.is_some_and(|ch| ch.is_alphanumeric() || "_./\\-".contains(ch));
        let glued_after = after.is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-');
        if !glued_before && !glued_after {
            return true;
        }
        from = end;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_keeps_its_spinner_and_its_rest_prefix() {
        assert_eq!(
            signal_of(HarnessId::Claude, "⠋ Claude Code"),
            Some(TitleSignal::Working)
        );
        assert_eq!(
            signal_of(HarnessId::Claude, ". Fixing the build"),
            Some(TitleSignal::Working)
        );
        assert_eq!(
            signal_of(HarnessId::Claude, "✳ Claude Code"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(
            signal_of(HarnessId::Claude, "* Claude Code"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(signal_of(HarnessId::Claude, "~/projects/repo"), None);
    }

    #[test]
    fn codex_is_read_by_its_name_and_a_state_word() {
        assert_eq!(
            signal_of(HarnessId::Codex, "codex working"),
            Some(TitleSignal::Working)
        );
        assert_eq!(
            signal_of(HarnessId::Codex, "codex ready"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(
            signal_of(HarnessId::Codex, "codex waiting for approval"),
            Some(TitleSignal::NeedsYou)
        );
        // A path that happens to contain both words is not a state.
        assert_eq!(signal_of(HarnessId::Codex, "~/codex/ready"), None);
        assert_eq!(signal_of(HarnessId::Codex, "codex"), None);
        assert_eq!(signal_of(HarnessId::Codex, "reworking codex"), None);
    }

    #[test]
    fn opencode_titles_are_its_oc_label_and_a_spinner_makes_them_work() {
        assert_eq!(
            signal_of(HarnessId::Opencode, "OC | build the app"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(
            signal_of(HarnessId::Opencode, "▣ OC | build the app"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(
            signal_of(HarnessId::Opencode, "⠋ OC | build the app"),
            Some(TitleSignal::Working)
        );
        assert_eq!(signal_of(HarnessId::Opencode, "OC |"), None);
    }

    #[test]
    fn omp_writes_its_state_after_the_brand() {
        assert_eq!(
            signal_of(HarnessId::Omp, "π : my-project"),
            Some(TitleSignal::Working)
        );
        assert_eq!(
            signal_of(HarnessId::Omp, "π > my-project"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(
            signal_of(HarnessId::Omp, "π ! my-project"),
            Some(TitleSignal::NeedsYou)
        );
        assert_eq!(
            signal_of(HarnessId::Omp, "π ⠋ my-project"),
            Some(TitleSignal::Working)
        );
        assert_eq!(
            signal_of(HarnessId::Omp, "OMP > my-project"),
            Some(TitleSignal::Rest)
        );
        assert_eq!(signal_of(HarnessId::Omp, "π: my-project"), None);
    }
}
