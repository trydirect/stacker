//! Post-processing for compose YAML emitted by `serde_yaml`.
//!
//! `serde_yaml` emits YAML 1.2, where `2222:22` is an unambiguous string, so it
//! writes port mappings without quotes. Docker Compose reads YAML 1.1, where
//! the very same token is a *sexagesimal* (base-60) integer:
//!
//! ```text
//! 2222:22  ->  2222 * 60 + 22  =  133342
//! ```
//!
//! The deploy then dies on the target host with
//! `invalid containerPort: 133342`. Only mappings whose right-hand side is a
//! valid base-60 digit (0-59) are affected, which is why `8082:80` survives a
//! round-trip untouched while `2222:22` does not — a failure that looks
//! arbitrary until you do the arithmetic.
//!
//! Every code path that serializes a compose document with `serde_yaml`
//! re-arms this, so the emitted text is repaired in one place.

/// Re-quote block-sequence entries under `ports:` and `expose:` so the emitted
/// document survives a YAML 1.1 reader.
///
/// Entries already quoted are left alone, as are long-form mapping entries
/// (`- target: 80`), which must not be wrapped in quotes.
pub fn quote_port_entries(yaml: &str) -> String {
    let mut out = String::with_capacity(yaml.len() + 32);
    // Indentation of the `ports:` / `expose:` key whose sequence we are inside.
    let mut key_indent: Option<usize> = None;

    for line in yaml.split_inclusive('\n') {
        let (content, newline) = match line.strip_suffix('\n') {
            Some(rest) => (rest, "\n"),
            None => (line, ""),
        };
        let trimmed = content.trim_start();
        let indent = content.len() - trimmed.len();

        if trimmed.is_empty() {
            out.push_str(line);
            continue;
        }

        // A line left of the key ends the sequence it introduced. Equal
        // indentation does *not*: `serde_yaml` emits block sequences flush with
        // their key, so `ports:` and its `- ` entries share a column.
        let is_sequence_entry = trimmed.starts_with("- ");
        if key_indent.is_some_and(|key| indent < key || (indent == key && !is_sequence_entry)) {
            key_indent = None;
        }

        if key_indent.is_some() && is_sequence_entry {
            if let Some(item) = trimmed.strip_prefix("- ") {
                out.push_str(&content[..indent]);
                out.push_str("- ");
                out.push_str(&quote_if_needed(item));
                out.push_str(newline);
                continue;
            }
        } else if matches!(trimmed, "ports:" | "expose:") {
            key_indent = Some(indent);
        }

        out.push_str(line);
    }

    out
}

/// Quote one sequence entry unless it is already quoted or is a nested mapping.
fn quote_if_needed(item: &str) -> String {
    let item = item.trim_end();

    // Already quoted, or an empty entry — nothing to do.
    if item.is_empty() || item.starts_with('"') || item.starts_with('\'') {
        return item.to_string();
    }

    // Long-form entry (`- target: 80`). A colon *followed by a space* — or a
    // trailing colon — marks a mapping; a port mapping's colons never are.
    if item.contains(": ") || item.ends_with(':') {
        return item.to_string();
    }

    format!("\"{}\"", item.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact production failure: a GitLab deploy shipped `- 2222:22`
    /// unquoted and Docker read it as 133342.
    #[test]
    fn quotes_the_sexagesimal_port_that_broke_the_gitlab_deploy() {
        let yaml = "services:\n  app:\n    ports:\n    - 8082:80\n    - 2222:22\n";
        let fixed = quote_port_entries(yaml);

        assert!(fixed.contains("- \"2222:22\""), "got:\n{fixed}");
        assert!(fixed.contains("- \"8082:80\""), "got:\n{fixed}");

        // 2222 * 60 + 22 == 133342 — the number from the deploy log.
        assert_eq!(2222 * 60 + 22, 133342);
    }

    #[test]
    fn quoted_entries_are_left_alone() {
        let yaml = "    ports:\n    - \"8082:80\"\n    - '2222:22'\n";
        assert_eq!(quote_port_entries(yaml), yaml);
    }

    #[test]
    fn long_form_port_mappings_are_not_quoted() {
        let yaml = "    ports:\n    - target: 80\n      published: 8082\n";
        let fixed = quote_port_entries(yaml);
        assert!(
            !fixed.contains("\"target: 80\""),
            "mapping entries must stay mappings:\n{fixed}"
        );
        assert_eq!(fixed, yaml);
    }

    #[test]
    fn only_the_ports_sequence_is_touched() {
        let yaml = "services:\n  app:\n    ports:\n    - 2222:22\n    volumes:\n    - data:/var/lib\n    networks:\n    - app-network\n";
        let fixed = quote_port_entries(yaml);

        assert!(fixed.contains("- \"2222:22\""), "got:\n{fixed}");
        assert!(
            fixed.contains("- data:/var/lib"),
            "volumes must be untouched:\n{fixed}"
        );
        assert!(
            fixed.contains("- app-network"),
            "networks must be untouched:\n{fixed}"
        );
    }

    #[test]
    fn expose_entries_are_quoted_too() {
        let yaml = "    expose:\n    - 2222:22\n";
        assert!(quote_port_entries(yaml).contains("- \"2222:22\""));
    }

    #[test]
    fn the_sequence_ends_with_its_indentation() {
        let yaml = "services:\n  app:\n    ports:\n    - 2222:22\n  db:\n    image: postgres\n";
        let fixed = quote_port_entries(yaml);
        assert!(fixed.contains("    image: postgres"), "got:\n{fixed}");
        assert!(!fixed.contains("\"image"), "got:\n{fixed}");
    }

    /// The whole point is the round-trip, so assert on it end to end.
    #[test]
    fn survives_a_serde_yaml_round_trip() {
        let original = "services:\n  app:\n    ports:\n      - \"8082:80\"\n      - \"2222:22\"\n";
        let value: serde_yaml::Value = serde_yaml::from_str(original).unwrap();
        let emitted = serde_yaml::to_string(&value).unwrap();

        // serde_yaml drops the quotes, which is the defect being repaired.
        assert!(
            emitted.contains("- 2222:22"),
            "expected serde_yaml to strip quotes, got:\n{emitted}"
        );

        let repaired = quote_port_entries(&emitted);
        let reparsed: serde_yaml::Value = serde_yaml::from_str(&repaired).unwrap();
        assert_eq!(
            reparsed["services"]["app"]["ports"][1],
            serde_yaml::Value::String("2222:22".to_string()),
            "port must still read back as a string:\n{repaired}"
        );
    }
}
