use super::{AppResult, Cli, CompletionArgs, CompletionShell, Repository, TaskState, task_status};
use clap::{Command, CommandFactory};
use clap_complete::aot::{Shell, generate};
use std::io;

pub fn print_script(args: CompletionArgs) -> AppResult<()> {
    if args.static_only {
        let shell = match args.shell {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::PowerShell => Shell::PowerShell,
        };
        generate(shell, &mut Cli::command(), "tat", &mut io::stdout());
    } else {
        print!(
            "{}",
            match args.shell {
                CompletionShell::Bash => include_str!("completion/tat.bash"),
                CompletionShell::PowerShell => include_str!("completion/tat.ps1"),
            }
        );
    }
    Ok(())
}

pub fn print_candidates(words: &[String]) -> AppResult<()> {
    for (value, description) in candidates(words) {
        println!(
            "{}\t{}",
            value.replace(['\t', '\r', '\n'], " "),
            description.replace(['\t', '\r', '\n'], " ")
        );
    }
    Ok(())
}

fn candidates(words: &[String]) -> Vec<(String, String)> {
    let Some(current) = words.last() else {
        return Vec::new();
    };
    let prior = &words[..words.len() - 1];
    let root = Cli::command();
    let command_index = find_command_index(prior);
    let subcommand = command_index.and_then(|index| root.find_subcommand(&prior[index]));

    if let Some(option) = value_option(prior, command_index, current) {
        let (name, prefix, fragment) = option;
        return match name {
            "--parent" | "--blocks" | "--add-blocks" | "--remove-blocks" => {
                task_candidates(subcommand.map(Command::get_name), prefix, fragment, true)
            }
            "--type" | "-t" => fixed_candidates(
                &["bug", "feature", "task", "gate"],
                prefix,
                fragment,
                "Task type",
            ),
            "--color" => {
                fixed_candidates(&["auto", "always", "never"], prefix, fragment, "Color mode")
            }
            _ => Vec::new(),
        };
    }

    if current.starts_with('-') {
        return option_candidates(&root, subcommand, current);
    }
    let Some(index) = command_index else {
        return command_candidates(&root, current);
    };
    if prior[index] == "help" {
        return command_candidates(&root, current);
    }
    let Some(command) = subcommand else {
        return Vec::new();
    };
    let tokens = &prior[index + 1..];
    let positionals = positionals_before(tokens);
    if current.is_empty() && positionals > 0 {
        return option_candidates(&root, Some(command), current);
    }
    match command.get_name() {
        "list" if positionals == 0 => fixed_candidates(
            &[
                "ready",
                "blocked",
                "not-done",
                "done",
                "all",
                "unblocked",
                "active",
            ],
            "",
            current,
            "List mode",
        ),
        "completions" if positionals == 0 => {
            fixed_candidates(&["bash", "powershell"], "", current, "Shell")
        }
        "view" | "set" | "done" | "reopen" | "remove" if positionals == 0 => {
            task_candidates(Some(command.get_name()), "", current, false)
        }
        _ => Vec::new(),
    }
}

fn find_command_index(words: &[String]) -> Option<usize> {
    let mut skip_value = false;
    for (index, word) in words.iter().enumerate().skip(1) {
        if skip_value {
            skip_value = false;
        } else if word == "--color" {
            skip_value = true;
        } else if !word.starts_with('-') {
            return Some(index);
        }
    }
    None
}

fn value_option<'a>(
    prior: &'a [String],
    command_index: Option<usize>,
    current: &'a str,
) -> Option<(&'static str, &'a str, &'a str)> {
    for name in [
        "--parent",
        "--blocks",
        "--add-blocks",
        "--remove-blocks",
        "--type",
        "--color",
    ] {
        if let Some(value) = current
            .strip_prefix(name)
            .and_then(|value| value.strip_prefix('='))
        {
            return Some((name, &current[..name.len() + 1], value));
        }
    }
    if current.starts_with('-') {
        return None;
    }
    let last = prior.last()?.as_str();
    if matches!(
        last,
        "--parent" | "--blocks" | "--add-blocks" | "--remove-blocks" | "--type" | "-t" | "--color"
    ) {
        return Some((
            match last {
                "-t" => "-t",
                "--parent" => "--parent",
                "--blocks" => "--blocks",
                "--add-blocks" => "--add-blocks",
                "--remove-blocks" => "--remove-blocks",
                "--type" => "--type",
                _ => "--color",
            },
            "",
            current,
        ));
    }
    let index = command_index?;
    let trailing = &prior[index + 1..];
    let last_option = trailing.iter().rposition(|word| word.starts_with('-'))?;
    let name = trailing[last_option].as_str();
    if matches!(name, "--blocks" | "--add-blocks" | "--remove-blocks")
        && trailing[last_option + 1..]
            .iter()
            .all(|word| !word.starts_with('-'))
    {
        return Some((
            match name {
                "--blocks" => "--blocks",
                "--add-blocks" => "--add-blocks",
                _ => "--remove-blocks",
            },
            "",
            current,
        ));
    }
    None
}

fn positionals_before(words: &[String]) -> usize {
    let mut count = 0;
    let mut skip_value = false;
    for word in words {
        if skip_value {
            skip_value = false;
        } else if matches!(
            word.as_str(),
            "--color"
                | "--type"
                | "-t"
                | "--priority"
                | "-p"
                | "--parent"
                | "--blocks"
                | "--add-blocks"
                | "--remove-blocks"
                | "--description"
                | "--body"
                | "--body-file"
                | "--completion-notes"
                | "--completion-notes-file"
        ) {
            skip_value = true;
        } else if !word.starts_with('-') {
            count += 1;
        }
    }
    count
}

fn task_candidates(
    command: Option<&str>,
    prefix: &str,
    current: &str,
    multi: bool,
) -> Vec<(String, String)> {
    let Some(repository) = Repository::discover(false)
        .ok()
        .filter(|repo| repo.validate().is_ok())
    else {
        return Vec::new();
    };
    let (list_prefix, fragment) = if multi {
        current
            .rfind([',', '-'])
            .map_or(("", current), |index| current.split_at(index + 1))
    } else {
        ("", current)
    };
    let blocked = repository.blocked_ids();
    let mut tasks = repository
        .tasks
        .iter()
        .filter(|task| match command {
            Some("done") => task.state == TaskState::NotDone,
            Some("reopen") => task.state == TaskState::Done,
            _ => true,
        })
        .filter(|task| task.id.starts_with(fragment))
        .collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.id.cmp(&right.id));
    tasks
        .into_iter()
        .map(|task| {
            (
                format!("{prefix}{list_prefix}{}", task.id),
                format!(
                    "{} · {} · P{}",
                    task.description,
                    task_status(task, &blocked),
                    task.priority_text
                ),
            )
        })
        .collect()
}

fn fixed_candidates(
    values: &[&str],
    prefix: &str,
    current: &str,
    description: &str,
) -> Vec<(String, String)> {
    values
        .iter()
        .filter(|value| value.starts_with(current))
        .map(|value| (format!("{prefix}{value}"), description.to_owned()))
        .collect()
}

fn command_candidates(root: &Command, current: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for command in root.get_subcommands() {
        let help = command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default();
        for name in std::iter::once(command.get_name()).chain(command.get_visible_aliases()) {
            if name.starts_with(current) {
                result.push((name.to_owned(), help.clone()));
            }
        }
    }
    if "help".starts_with(current) {
        result.push(("help".to_owned(), "Show command help".to_owned()));
    }
    result
}

fn option_candidates(
    root: &Command,
    subcommand: Option<&Command>,
    current: &str,
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for command in [Some(root), subcommand].into_iter().flatten() {
        for arg in command.get_arguments() {
            let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
            if let Some(long) = arg.get_long() {
                let option = format!("--{long}");
                if option.starts_with(current) {
                    result.push((option, help.clone()));
                }
            }
            if let Some(short) = arg.get_short() {
                let option = format!("-{short}");
                if option.starts_with(current) {
                    result.push((option, help.clone()));
                }
            }
        }
    }
    for option in ["--help", "-h", "--version", "-V"] {
        if option.starts_with(current) {
            result.push((option.to_owned(), "Built-in option".to_owned()));
        }
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    result.dedup_by(|left, right| left.0 == right.0);
    result
}
