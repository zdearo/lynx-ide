//! Turns the page's accessibility tree into the text an agent reads. Each element it can act on
//! gets a short ref (`e12`) that stays the same for as long as the element lives, so an agent can
//! take a snapshot, act, and take another without its refs changing under it.

use std::collections::HashMap;
use std::fmt::Write;

use serde_json::Value;

/// Past this the snapshot stops, so one huge page cannot fill an agent's context.
const MAX_CHARS: usize = 40_000;
const MAX_NAME_CHARS: usize = 160;
const MAX_URL_CHARS: usize = 200;

/// Roles an agent can click, type into or choose, which therefore get a ref.
const ACTIONABLE: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "combobox",
    "checkbox",
    "radio",
    "switch",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "slider",
    "spinbutton",
    "treeitem",
];

/// Roles that only wrap other content: they are walked through, never printed.
const TRANSPARENT: &[&str] = &[
    "RootWebArea",
    "WebArea",
    "generic",
    "none",
    "presentation",
    "InlineTextBox",
    "LineBreak",
    "paragraph",
    "Section",
    "group",
    "list",
    "listitem",
    "DescriptionList",
    "term",
    "definition",
];

/// Refs by the page's own node numbers. The node number never repeats within a page's life, so a
/// ref for a node that is gone fails cleanly instead of pointing at something else.
#[derive(Default)]
pub struct RefTable {
    next: u32,
    by_node: HashMap<i64, u32>,
    by_ref: HashMap<u32, i64>,
    /// What each element is called on the page, for telling the person what the agent is doing.
    names: HashMap<i64, String>,
}

impl RefTable {
    fn ref_for(&mut self, node: i64) -> String {
        let number = *self.by_node.entry(node).or_insert_with(|| {
            self.next += 1;
            self.next
        });
        self.by_ref.insert(number, node);
        format!("e{number}")
    }

    /// The name the page gives the element a ref stands for, if it has one.
    pub fn name_of(&self, reference: &str) -> Option<&str> {
        let node = self.node_of(reference)?;
        self.names.get(&node).map(String::as_str)
    }

    /// The page node a ref stands for, or None for a ref this table never handed out.
    pub fn node_of(&self, reference: &str) -> Option<i64> {
        let number = reference.trim().trim_start_matches('@').strip_prefix('e')?;
        self.by_ref.get(&number.parse().ok()?).copied()
    }
}

struct Walk<'a> {
    nodes: HashMap<&'a str, &'a Value>,
    refs: &'a mut RefTable,
    out: String,
    truncated: bool,
}

/// The snapshot text for the tree CDP's `Accessibility.getFullAXTree` returned.
pub fn render(tree: &Value, refs: &mut RefTable) -> String {
    let Some(list) = tree.get("nodes").and_then(Value::as_array) else {
        return String::new();
    };
    let nodes: HashMap<&str, &Value> = list
        .iter()
        .filter_map(|node| Some((node.get("nodeId")?.as_str()?, node)))
        .collect();
    // The root is the one node nothing lists as its child.
    let root = list.iter().find(|node| node.get("parentId").is_none());
    let mut walk = Walk {
        nodes,
        refs,
        out: String::new(),
        truncated: false,
    };
    if let Some(root) = root {
        walk.visit(root, 0, false);
    }
    if walk.truncated {
        walk.out
            .push_str("(snapshot cut short: the page has more; scroll to see the rest)\n");
    }
    walk.out
}

impl Walk<'_> {
    fn visit(&mut self, node: &Value, depth: usize, text_is_covered: bool) {
        if self.truncated {
            return;
        }
        let role = text_of(node.get("role"));
        let ignored = node.get("ignored").and_then(Value::as_bool) == Some(true);
        let name = text_of(node.get("name"));
        let mut child_depth = depth;
        let mut covered = text_is_covered;

        if !ignored && let Some(line) = self.line(node, &role, &name, text_is_covered) {
            if self.out.len() + line.len() + depth * 2 > MAX_CHARS {
                self.truncated = true;
                return;
            }
            let _ = writeln!(self.out, "{}{line}", "  ".repeat(depth));
            child_depth = depth + 1;
            // What a button or link says is already in its name; do not repeat it as text.
            covered = !name.is_empty();
        }

        let children = node
            .get("childIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str);
        for id in children {
            if let Some(child) = self.nodes.get(id).copied() {
                self.visit(child, child_depth, covered);
            }
        }
    }

    /// The line for a node, or None when it is only walked through.
    fn line(&mut self, node: &Value, role: &str, name: &str, covered: bool) -> Option<String> {
        if role == "StaticText" {
            return (!covered && !name.trim().is_empty())
                .then(|| format!("- text \"{}\"", clip(name, MAX_NAME_CHARS)));
        }
        let actionable = ACTIONABLE.contains(&role);
        if TRANSPARENT.contains(&role) || role.is_empty() || (name.is_empty() && !actionable) {
            return None;
        }
        let mut line = format!("- {role}");
        if !name.is_empty() {
            let _ = write!(line, " \"{}\"", clip(name, MAX_NAME_CHARS));
        }
        if let Some(backend) = node.get("backendDOMNodeId").and_then(Value::as_i64)
            && actionable
        {
            let _ = write!(line, " [ref={}]", self.refs.ref_for(backend));
            if !name.is_empty() {
                self.refs.names.insert(backend, clip(name, MAX_NAME_CHARS));
            }
        }
        for flag in properties(node) {
            let _ = write!(line, " [{flag}]");
        }
        if let Some(value) = node.get("value").map(|value| text_of(Some(value)))
            && !value.is_empty()
        {
            let _ = write!(line, " value=\"{}\"", clip(&value, MAX_NAME_CHARS));
        }
        Some(line)
    }
}

/// The states worth telling an agent about, as `name` or `name=value`.
fn properties(node: &Value) -> Vec<String> {
    let mut flags = Vec::new();
    let list = node.get("properties").and_then(Value::as_array);
    for property in list.into_iter().flatten() {
        let (Some(name), Some(value)) = (
            property.get("name").and_then(Value::as_str),
            property.get("value").and_then(|value| value.get("value")),
        ) else {
            continue;
        };
        match (name, value) {
            ("level", Value::Number(level)) => flags.push(format!("level={level}")),
            ("checked" | "pressed", Value::String(state)) if state != "false" => {
                flags.push(format!("{name}={state}"));
            }
            ("disabled" | "selected" | "focused" | "required", Value::Bool(true)) => {
                flags.push(name.to_owned());
            }
            ("expanded", Value::Bool(open)) => flags.push(format!("expanded={open}")),
            ("url", Value::String(url)) => flags.push(format!("href={}", clip(url, MAX_URL_CHARS))),
            _ => {}
        }
    }
    flags
}

/// The `value` of an AX value object (`{"type": "...", "value": "..."}`) as text.
fn text_of(value: Option<&Value>) -> String {
    match value.and_then(|value| value.get("value")) {
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn clip(text: &str, limit: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= limit {
        return single_line;
    }
    let cut: String = single_line.chars().take(limit).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn value(text: &str) -> Value {
        json!({ "type": "computedString", "value": text })
    }

    fn node(id: &str, parent: Option<&str>, role: &str, name: &str, children: &[&str]) -> Value {
        let mut node = json!({
            "nodeId": id,
            "ignored": false,
            "role": { "type": "role", "value": role },
            "name": value(name),
            "childIds": children,
            "backendDOMNodeId": id.parse::<i64>().unwrap() * 10,
        });
        if let Some(parent) = parent {
            node["parentId"] = json!(parent);
        }
        node
    }

    fn sample() -> Value {
        let mut heading = node("2", Some("1"), "heading", "Sign in", &["3"]);
        heading["properties"] =
            json!([{ "name": "level", "value": { "type": "integer", "value": 1 } }]);
        let mut email = node("4", Some("1"), "textbox", "Email", &[]);
        email["value"] = value("ana@example.com");
        let mut link = node("5", Some("1"), "link", "Forgot it?", &["6"]);
        link["properties"] = json!([{ "name": "url", "value": { "type": "string", "value": "https://x.test/reset" } }]);
        let mut hidden = node("8", Some("1"), "button", "Never shown", &[]);
        hidden["ignored"] = json!(true);
        json!({ "nodes": [
            node("1", None, "RootWebArea", "Login", &["2", "4", "5", "7", "8"]),
            heading,
            node("3", Some("2"), "StaticText", "Sign in", &[]),
            email,
            link,
            node("6", Some("5"), "StaticText", "Forgot it?", &[]),
            node("7", Some("1"), "StaticText", "Welcome back", &[]),
            hidden,
        ]})
    }

    #[test]
    fn actionable_elements_get_refs_and_text_is_not_repeated() {
        let mut refs = RefTable::default();
        let text = render(&sample(), &mut refs);

        assert_eq!(
            text,
            "- heading \"Sign in\" [level=1]\n\
             - textbox \"Email\" [ref=e1] value=\"ana@example.com\"\n\
             - link \"Forgot it?\" [ref=e2] [href=https://x.test/reset]\n\
             - text \"Welcome back\"\n"
        );
    }

    #[test]
    fn refs_stay_the_same_between_snapshots_and_map_back_to_nodes() {
        let mut refs = RefTable::default();
        render(&sample(), &mut refs);
        let second = render(&sample(), &mut refs);

        assert!(second.contains("[ref=e1]"));
        assert_eq!(refs.node_of("e1"), Some(40));
        assert_eq!(refs.node_of("@e2"), Some(50));
        assert_eq!(refs.node_of("e9"), None);
        assert_eq!(refs.name_of("e1"), Some("Email"));
        assert_eq!(refs.name_of("e9"), None);
        assert_eq!(refs.node_of("nonsense"), None);
    }

    #[test]
    fn a_page_larger_than_the_limit_is_cut_with_a_note() {
        let mut nodes = vec![node("1", None, "RootWebArea", "Big", &[])];
        let mut ids = Vec::new();
        for index in 2..2000 {
            let id = index.to_string();
            nodes.push(node(&id, Some("1"), "button", &"x".repeat(100), &[]));
            ids.push(id);
        }
        nodes[0]["childIds"] = json!(ids);
        let mut refs = RefTable::default();

        let text = render(&json!({ "nodes": nodes }), &mut refs);

        assert!(text.len() < MAX_CHARS + 200);
        assert!(text.ends_with("scroll to see the rest)\n"));
    }

    #[test]
    fn an_empty_answer_gives_an_empty_snapshot() {
        let mut refs = RefTable::default();
        assert_eq!(render(&json!({}), &mut refs), "");
    }
}
