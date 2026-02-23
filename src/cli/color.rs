//! Syntax-highlighted YAML output for the `parse` subcommand.

use colored::Colorize;

pub(super) fn print_colored_yaml(yaml: &str) {
    for line in yaml.lines() {
        let trimmed = line.trim_start();

        let indent = &line[..line.len() - trimmed.len()];

        // List item prefix
        let (prefix, rest) = if let Some(stripped) = trimmed.strip_prefix("- ") {
            ("- ", stripped)
        } else {
            ("", trimmed)
        };

        // Check if this is a key: value line
        if let Some(colon_pos) = find_yaml_key_end(rest) {
            let key = &rest[..colon_pos];
            let after_colon = &rest[colon_pos..];

            print!("{}{}", indent, prefix.dimmed());
            print!("{}", key.cyan());

            if after_colon.len() > 1 {
                let value = after_colon[1..].trim_start();
                print!("{}", ":".dimmed());
                if !value.is_empty() {
                    print!(" {}", colorize_yaml_value(value));
                }
            } else {
                print!("{}", after_colon.dimmed());
            }
            println!();
        } else {
            print!("{}{}", indent, prefix.dimmed());
            println!("{}", colorize_yaml_value(rest));
        }
    }
}

fn find_yaml_key_end(s: &str) -> Option<usize> {
    let mut in_quote = false;
    for (i, c) in s.char_indices() {
        match c {
            '\'' | '"' => in_quote = !in_quote,
            ':' if !in_quote => {
                let next = s[i + 1..].chars().next();
                if next.is_none() || next == Some(' ') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn colorize_yaml_value(value: &str) -> String {
    if value == "true" || value == "false" {
        value.yellow().bold().to_string()
    } else if value == "~" || value == "null" {
        value.dimmed().to_string()
    } else if value.starts_with('!') {
        value.magenta().bold().to_string()
    } else if value.parse::<i64>().is_ok() {
        value.magenta().to_string()
    } else {
        value.green().to_string()
    }
}
