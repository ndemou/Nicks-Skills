mod support;

use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::UNIX_EPOCH;
use support::TestDir;

fn repository() -> TestDir {
    let directory = support::tempdir();
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(directory.path())
        .status()
        .expect("run git init");
    assert!(status.success());
    directory
}

fn tat(repository: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tat"))
        .args(args)
        .current_dir(repository)
        .output()
        .expect("run tat")
}

fn success(repository: &Path, args: &[&str]) -> String {
    let output = tat(repository, args);
    assert!(
        output.status.success(),
        "tat {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 stdout")
        .trim()
        .to_owned()
}

fn failure(repository: &Path, args: &[&str]) -> (String, String) {
    let output = tat(repository, args);
    assert!(
        !output.status.success(),
        "tat {args:?} unexpectedly succeeded"
    );
    (
        String::from_utf8(output.stdout).expect("UTF-8 stdout"),
        String::from_utf8(output.stderr).expect("UTF-8 stderr"),
    )
}

fn assert_layered_diagnostic(stderr: &str, summary: &str) {
    assert!(
        stderr.starts_with(&format!("error: {summary}\n\n")),
        "diagnostic did not start with the expected expert summary:\n{stderr}"
    );
    assert!(
        stderr.contains("\n\ndetails:\n"),
        "diagnostic omitted technical details:\n{stderr}"
    );
    assert!(
        stderr.contains("\n\ntry: "),
        "diagnostic omitted a recovery action:\n{stderr}"
    );
}

fn created_path(repository: &Path, args: &[&str]) -> PathBuf {
    PathBuf::from(success(repository, args))
}

fn id_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .expect("UTF-8 filename")
        .split(',')
        .next()
        .expect("task ID")
        .to_owned()
}

fn table_has_id(table: &str, id: &str) -> bool {
    table
        .lines()
        .any(|line| line.split('|').nth(1).is_some_and(|cell| cell.trim() == id))
}

fn json_success(repository: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&success(repository, args)).expect("valid JSON output")
}

fn json_task<'a>(document: &'a Value, id: &str) -> &'a Value {
    document["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["id"] == id)
        .unwrap_or_else(|| panic!("JSON task {id} not found"))
}

fn canonical_blocks(ids: &[&str]) -> String {
    let mut ids = ids.to_vec();
    ids.sort();
    format!("blocks-{}", ids.join("-"))
}

fn task_metadata_and_body(path: &Path) -> (Value, String) {
    let stored = fs::read_to_string(path).expect("task text");
    let after_prefix = stored
        .strip_prefix("<!-- tat-metadata: ")
        .expect("formal task metadata");
    let (metadata_line, remainder) = after_prefix.split_once('\n').expect("metadata line");
    let metadata_json = metadata_line
        .trim_end_matches('\r')
        .strip_suffix(" -->")
        .expect("metadata comment suffix");
    let body = remainder
        .strip_prefix("\r\n")
        .or_else(|| remainder.strip_prefix('\n'))
        .unwrap_or(remainder)
        .to_owned();
    (
        serde_json::from_str(metadata_json).expect("metadata JSON"),
        body,
    )
}

fn canonical_utc_to_unix_ms(value: &str) -> u64 {
    assert_eq!(value.len(), 24, "canonical UTC timestamp length");
    let number = |start: usize, end: usize| value[start..end].parse::<i64>().unwrap();
    let year = number(0, 4);
    let month = number(5, 7);
    let day = number(8, 10);
    let hour = number(11, 13);
    let minute = number(14, 16);
    let second = number(17, 19);
    let millisecond = number(20, 23);
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let month_piece = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_piece + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(
        days * 86_400_000 + hour * 3_600_000 + minute * 60_000 + second * 1_000 + millisecond,
    )
    .unwrap()
}

#[test]
fn creates_repository_local_ids_and_omits_empty_relationship_fields() {
    let first = repository();
    let second = repository();
    let id_pattern = Regex::new(r"^t@[a-z0-9]{4}$").unwrap();

    let mut first_ids = Vec::new();
    for number in 0..24 {
        let path = created_path(first.path(), &["new", &format!("First task {number}")]);
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(!filename.contains(", parent"));
        assert!(!filename.contains(", blocks"));
        first_ids.push(id_from_path(&path));
    }
    let second_path = created_path(second.path(), &["new", "Second repository task"]);
    let second_id = id_from_path(&second_path);

    assert!(first_ids.iter().all(|id| id_pattern.is_match(id)));
    assert!(id_pattern.is_match(&second_id));
    first_ids.sort();
    first_ids.dedup();
    assert_eq!(
        first_ids.len(),
        24,
        "IDs must be unique within a repository"
    );
    assert_eq!(
        fs::read_dir(first.path().join("tasks")).unwrap().count(),
        25
    );
    assert_eq!(
        fs::read_dir(second.path().join("tasks")).unwrap().count(),
        2
    );
}

#[test]
fn task_type_values_accept_uppercase_and_mixed_case_without_changing_task_text() {
    let repo = repository();
    let title = "Keep API and MiXeD title Case";
    let body = "# API Details\n\nPreserve MiXeD body Case and BUG text.";

    for (canonical, mixed) in [
        ("BUG", "bUg"),
        ("FEATURE", "FeAtUrE"),
        ("TASK", "tAsK"),
        ("GATE", "GaTe"),
    ] {
        for value in [canonical, mixed] {
            let path = created_path(
                repo.path(),
                &["new", title, "--type", value, "--body", body],
            );
            let id = id_from_path(&path);
            assert_eq!(
                path.file_name().unwrap().to_str().unwrap(),
                format!("{id}, p5, {canonical}, {title}.md")
            );
            let document = json_success(repo.path(), &["list", "all", "--json"]);
            let task = json_task(&document, &id);
            assert_eq!(task["type"], canonical);
            assert_eq!(task["title"], title);
            assert_eq!(task["body_markdown"], body);

            let initial_kind = if canonical == "TASK" { "bug" } else { "task" };
            let original_path = created_path(
                repo.path(),
                &["new", title, "--type", initial_kind, "--body", body],
            );
            let updated_id = id_from_path(&original_path);
            let updated_path = created_path(repo.path(), &["set", &updated_id, "-t", value]);
            assert!(!original_path.exists());
            assert_eq!(
                updated_path.file_name().unwrap().to_str().unwrap(),
                format!("{updated_id}, p5, {canonical}, {title}.md")
            );
            let document = json_success(repo.path(), &["list", "all", "--json"]);
            let task = json_task(&document, &updated_id);
            assert_eq!(task["type"], canonical);
            assert_eq!(task["title"], title);
            assert_eq!(task["body_markdown"], body);
        }
    }
}

#[test]
fn list_mode_values_and_aliases_accept_uppercase_and_mixed_case() {
    let repo = repository();
    let blocked = id_from_path(&created_path(repo.path(), &["new", "Blocked work"]));
    created_path(repo.path(), &["new", "Ready blocker", "--blocks", &blocked]);
    let done = id_from_path(&created_path(repo.path(), &["new", "Finished work"]));
    success(repo.path(), &["done", &done]);

    for (lowercase, mixed) in [
        ("ready", "ReAdY"),
        ("unblocked", "UnBlOcKeD"),
        ("active", "AcTiVe"),
        ("blocked", "BlOcKeD"),
        ("not-done", "NoT-DoNe"),
        ("done", "DoNe"),
        ("all", "AlL"),
    ] {
        let expected = json_success(repo.path(), &["list", lowercase, "--json"]);
        assert!(!expected["tasks"].as_array().unwrap().is_empty());
        for value in [lowercase.to_ascii_uppercase(), mixed.to_owned()] {
            assert_eq!(
                json_success(repo.path(), &["list", &value, "--json"]),
                expected,
                "list mode {value:?} changed the result"
            );
        }
    }
}

#[test]
fn color_values_accept_uppercase_and_mixed_case_for_output_and_parse_errors() {
    let repo = repository();
    created_path(repo.path(), &["new", "Color case test"]);

    for (lowercase, mixed) in [("auto", "AuTo"), ("always", "AlWaYs"), ("never", "NeVeR")] {
        let expected = success(repo.path(), &["list", "--color", lowercase]);
        let (_, expected_error) = failure(repo.path(), &["--color", lowercase, "done"]);
        assert_eq!(expected.contains("\x1b["), lowercase == "always");
        assert_eq!(expected_error.contains("\x1b["), lowercase == "always");

        for value in [lowercase.to_ascii_uppercase(), mixed.to_owned()] {
            assert_eq!(success(repo.path(), &["list", "--color", &value]), expected);
            let (_, error) = failure(repo.path(), &["--color", &value, "done"]);
            assert_eq!(error, expected_error);
            let inline = format!("--color={value}");
            assert_eq!(success(repo.path(), &[&inline, "list"]), expected);
            let (_, error) = failure(repo.path(), &[&inline, "done"]);
            assert_eq!(error, expected_error);
        }
    }
}

#[test]
fn unknown_enum_values_remain_errors_without_writing_tasks() {
    let repo = repository();
    for args in [
        vec!["new", "Do not create", "--type", "BuGs"],
        vec!["new", "Do not create", "--color", "RaInBoW"],
        vec!["list", "ReAdY-IsH"],
    ] {
        let (stdout, stderr) = failure(repo.path(), &args);
        assert!(stdout.is_empty());
        assert!(stderr.contains("invalid value"), "{stderr}");
        assert!(!repo.path().join("tasks").exists());
    }

    let path = created_path(
        repo.path(),
        &["new", "Unchanged API Case", "--body", "MiXeD body"],
    );
    let id = id_from_path(&path);
    let original_bytes = fs::read(&path).unwrap();
    let original_listing = json_success(repo.path(), &["list", "all", "--json"]);
    let (stdout, stderr) = failure(repo.path(), &["set", &id, "--type", "TaSkS"]);
    assert!(stdout.is_empty());
    assert!(stderr.contains("invalid value"), "{stderr}");
    assert_eq!(fs::read(path).unwrap(), original_bytes);
    assert_eq!(
        json_success(repo.path(), &["list", "all", "--json"]),
        original_listing
    );
}

#[test]
fn extracts_ids_from_filenames_and_paths_without_repository_discovery() {
    let repo = repository();
    let path = created_path(repo.path(), &["new", "Path conversion task"]);
    let expected = id_from_path(&path);
    let outside = support::tempdir();
    let full_path = path.to_str().expect("UTF-8 task path");
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("UTF-8 task filename");

    assert_eq!(success(outside.path(), &["id", full_path]), expected);
    assert_eq!(success(outside.path(), &["id", filename]), expected);

    let imaginary = outside.path().join(filename);
    assert!(!imaginary.exists());
    assert_eq!(
        success(outside.path(), &["id", imaginary.to_str().unwrap()]),
        expected
    );

    let (stdout, stderr) = failure(outside.path(), &["id", "not-a-task.md"]);
    assert!(stdout.is_empty());
    assert_layered_diagnostic(&stderr, "A task filename is invalid.");
    assert!(stderr.contains("task filename does not follow the required format"));
}

#[test]
fn returns_the_first_ready_id_and_explains_when_none_is_available() {
    let repo = repository();
    let later = id_from_path(&created_path(
        repo.path(),
        &["new", "Later", "--priority", "3"],
    ));
    let equal_a = id_from_path(&created_path(
        repo.path(),
        &["new", "Equal A", "--priority", "1"],
    ));
    let equal_b = id_from_path(&created_path(
        repo.path(),
        &["new", "Equal B", "--priority", "1"],
    ));
    let expected = std::cmp::min(equal_a.as_str(), equal_b.as_str());
    assert_eq!(success(repo.path(), &["next"]), expected);

    success(repo.path(), &["done", &equal_a]);
    success(repo.path(), &["done", &equal_b]);
    assert_eq!(success(repo.path(), &["next"]), later);
    success(repo.path(), &["done", &later]);

    let (stdout, stderr) = failure(repo.path(), &["next"]);
    assert!(stdout.is_empty());
    assert_layered_diagnostic(&stderr, "No ready task is available.");
    assert!(stderr.contains("Either all remaining work is blocked, or every task is already done"));
    assert!(stderr.contains("tat list blocked"));
    assert!(stderr.contains("tat list not-done"));
}

#[test]
fn accepts_comma_hyphen_whitespace_and_mixed_block_id_separators() {
    let repo = repository();
    let target_a = id_from_path(&created_path(repo.path(), &["new", "Target A"]));
    let target_b = id_from_path(&created_path(repo.path(), &["new", "Target B"]));
    let target_c = id_from_path(&created_path(repo.path(), &["new", "Target C"]));
    let target_d = id_from_path(&created_path(repo.path(), &["new", "Target D"]));

    let comma_value = format!("{target_a},{target_b}");
    let comma_path = created_path(
        repo.path(),
        &["new", "Comma-separated gate", "--blocks", &comma_value],
    );
    assert!(
        comma_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&canonical_blocks(&[&target_a, &target_b]))
    );

    let hyphen_value = format!("{target_a}-{target_b}");
    let hyphen_path = created_path(
        repo.path(),
        &["new", "Hyphen-separated gate", "--blocks", &hyphen_value],
    );
    assert!(
        hyphen_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&canonical_blocks(&[&target_a, &target_b]))
    );

    let space_value = format!("{target_a} {target_b}");
    let space_path = created_path(
        repo.path(),
        &["new", "Space-separated gate", "--blocks", &space_value],
    );
    assert!(
        space_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&canonical_blocks(&[&target_a, &target_b]))
    );

    let separate_values_path = created_path(
        repo.path(),
        &[
            "new",
            "Separate argument gate",
            "--blocks",
            &target_a,
            &target_b,
        ],
    );
    let gate = id_from_path(&separate_values_path);
    let mixed_value = format!("{target_a}, {target_b}-{target_c}  {target_d},{target_a}");
    let mixed_path = created_path(repo.path(), &["set", &gate, "--blocks", &mixed_value]);
    assert!(
        mixed_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&canonical_blocks(&[
                &target_a, &target_b, &target_c, &target_d
            ]))
    );

    let remove_value = format!("{target_a} {target_d}");
    let removed_path = created_path(
        repo.path(),
        &["set", &gate, "--remove-blocks", &remove_value],
    );
    assert!(
        removed_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&canonical_blocks(&[&target_b, &target_c]))
    );

    let add_value = format!("{target_a}-{target_d}");
    let restored_path = created_path(repo.path(), &["set", &gate, "--add-blocks", &add_value]);
    assert!(
        restored_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&canonical_blocks(&[
                &target_a, &target_b, &target_c, &target_d
            ]))
    );

    let (_, error) = failure(
        repo.path(),
        &["new", "Separator without IDs", "--blocks", "-"],
    );
    assert!(error.contains("task ID list must contain at least one ID"));
}

#[test]
fn supports_the_full_task_lifecycle_and_list_formats() {
    let repo = repository();
    let root_path = created_path(
        repo.path(),
        &["new", "Root work", "--priority", "3", "--type", "feature"],
    );
    let root = id_from_path(&root_path);
    let child_path = created_path(
        repo.path(),
        &[
            "new",
            "Child work",
            "--parent",
            &root,
            "--body",
            "Initial notes",
        ],
    );
    let child = id_from_path(&child_path);
    let (created_metadata, created_body) = task_metadata_and_body(&child_path);
    assert_eq!(created_metadata["version"], 2);
    let child_created_at = created_metadata["created_at"]
        .as_str()
        .expect("creation timestamp");
    assert!(child_created_at.ends_with('Z'));
    assert!(created_metadata.get("completed_at").is_none());
    assert!(
        created_metadata["completions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(created_body, "Initial notes");
    let grandchild_path =
        created_path(repo.path(), &["new", "Grandchild work", "--parent", &child]);
    let grandchild = id_from_path(&grandchild_path);
    let gate_path = created_path(
        repo.path(),
        &["new", "Release gate", "--type", "gate", "--blocks", &child],
    );
    let gate = id_from_path(&gate_path);

    assert!(
        child_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&format!("parent-{root}"))
    );
    let gate_path = created_path(repo.path(), &["set", &gate, "--add-blocks", &grandchild]);
    let mut blocked_ids = [&child, &grandchild];
    blocked_ids.sort();
    assert!(
        gate_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&format!("blocks-{}-{}", blocked_ids[0], blocked_ids[1]))
    );

    let ready = success(repo.path(), &["list"]);
    let header = ready.lines().next().unwrap();
    let header_cells = header.split('|').map(str::trim).collect::<Vec<_>>();
    assert_eq!(header_cells.get(1), Some(&"ID"));
    assert_eq!(header_cells.get(2), Some(&"Description"));
    assert_eq!(header_cells.get(3), Some(&"Prio"));
    assert!(!header.contains("Priority"));
    assert!(ready.contains(&root));
    assert!(ready.contains(&gate));
    assert!(!table_has_id(&ready, &child));
    assert!(!table_has_id(&ready, &grandchild));
    assert_eq!(ready, success(repo.path(), &["list", "unblocked"]));
    assert_eq!(ready, success(repo.path(), &["list", "active"]));
    assert_eq!(ready, success(repo.path(), &["ls", "--Markdown"]));
    let not_done = success(repo.path(), &["list", "not-done"]);
    assert!(table_has_id(&not_done, &root));
    assert!(table_has_id(&not_done, &child));
    assert!(table_has_id(&not_done, &grandchild));
    assert!(table_has_id(&not_done, &gate));
    assert!(!not_done.lines().next().unwrap().contains("State"));
    assert!(not_done.lines().next().unwrap().contains("Status"));

    let filenames = success(repo.path(), &["list", "ready", "--filenames-output"]);
    assert!(filenames.lines().all(|line| line.ends_with(".md")));
    assert!(!filenames.contains("| ID |"));

    let (_, blocked_error) = failure(repo.path(), &["complete", &child]);
    assert!(blocked_error.contains("is blocked"));
    assert!(blocked_error.contains(&format!("task '{gate}' directly blocks '{child}'")));
    let (_, descendants_error) = failure(repo.path(), &["done", &root]);
    assert!(descendants_error.contains("not-done descendants"));

    success(repo.path(), &["done", &gate]);
    let now_ready = success(repo.path(), &["list", "ready"]);
    assert!(now_ready.contains(&child));
    assert!(now_ready.contains(&grandchild));

    success(
        repo.path(),
        &[
            "update",
            &child,
            "--description",
            "Updated child",
            "--priority",
            "2.5",
            "--type",
            "bug",
            "--body",
            "Updated notes",
        ],
    );
    let view = success(repo.path(), &["show", &child]);
    assert!(view.contains("Updated child"));
    assert!(view.contains("Updated notes"));
    assert!(!view.contains("| State |"));
    assert!(view.contains("| Status | ready |"));
    assert!(view.contains("| Priority | 2.5 |"));
    assert!(view.contains("| Type | BUG |"));

    success(
        repo.path(),
        &[
            "done",
            &grandchild,
            "--done-notes",
            "Implemented the leaf and verified its focused test.",
        ],
    );
    let child_done_path = PathBuf::from(success(
        repo.path(),
        &[
            "done",
            &child,
            "--completion-notes",
            "Implemented retry handling.\n\n**Verification:** unit and timeout tests pass.",
        ],
    ));
    let (done_metadata, done_body) = task_metadata_and_body(&child_done_path);
    assert_eq!(done_metadata["created_at"], child_created_at);
    assert!(done_metadata.get("completed_at").is_none());
    let completed_at = done_metadata["completions"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .expect("completion timestamp");
    assert!(completed_at >= child_created_at);
    assert!(done_body.starts_with(&format!(
        "Updated notes\n\n## Completion notes — {completed_at}\n\n"
    )));
    assert!(done_body.contains("Implemented retry handling."));
    assert!(done_body.contains("**Verification:** unit and timeout tests pass."));
    let modified_at = fs::metadata(&child_done_path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(modified_at.abs_diff(canonical_utc_to_unix_ms(completed_at)) <= 2);
    success(repo.path(), &["done", &root]);
    let completed = success(repo.path(), &["list", "done"]);
    assert!(completed.contains(&root));
    assert!(completed.contains(&child));
    assert!(completed.contains(&grandchild));
    assert!(completed.contains(&gate));
    let done_view = success(repo.path(), &["view", &root]);
    assert!(!done_view.contains("| State |"));
    assert!(done_view.contains("| Status | done |"));
    let child_done_view = success(repo.path(), &["view", &child]);
    assert!(child_done_view.contains("## Completion notes"));
    assert!(!child_done_view.contains("tat-metadata"));

    success(repo.path(), &["reopen", &root]);
    let root_reopened_path =
        PathBuf::from(success(repo.path(), &["set", &root, "--priority", "3.5"]));
    let (reopened_metadata, _) = task_metadata_and_body(&root_reopened_path);
    assert!(reopened_metadata.get("completed_at").is_none());
    assert_eq!(
        reopened_metadata["completions"].as_array().unwrap().len(),
        1
    );
    assert!(success(repo.path(), &["list", "not-done"]).contains(&root));
    assert_eq!(
        success(repo.path(), &["list", "active"]),
        success(repo.path(), &["list", "ready"])
    );
    assert!(success(repo.path(), &["check"]).starts_with("OK:"));
    assert!(success(repo.path(), &["validate"]).starts_with("OK:"));
    let (graph_stdout, graph_stderr) = failure(repo.path(), &["graph", "--all"]);
    assert!(graph_stdout.is_empty());
    assert!(graph_stderr.contains("unrecognized subcommand 'graph'"));

    let disposable = id_from_path(&created_path(repo.path(), &["new", "Disposable"]));
    let (_, confirmation_error) = failure(repo.path(), &["remove", &disposable]);
    assert!(confirmation_error.contains("--force"));
    assert!(confirmation_error.contains(&format!("Run 'tat view {disposable}'")));
    assert!(confirmation_error.contains(&format!("tat remove {disposable} --force")));
    success(repo.path(), &["delete", &disposable, "--force"]);
    let (_, missing_error) = failure(repo.path(), &["view", &disposable]);
    assert!(missing_error.contains("does not exist"));
    assert!(missing_error.contains("git log --all --name-status -- tasks/ tasks/done/"));
}

#[test]
fn upgrades_v1_metadata_and_migrates_legacy_files_without_inventing_time() {
    let repo = repository();
    let tasks = repo.path().join("tasks");
    let done = tasks.join("done");
    fs::create_dir_all(&done).unwrap();
    let legacy_path = tasks.join("t@lgcy, p5, TASK, Legacy active task.md");
    fs::write(&legacy_path, "Legacy body\n").unwrap();
    let historical_path = done.join("t@hist, p5, TASK, Version one completed task.md");
    fs::write(
        &historical_path,
        "<!-- tat-metadata: {\"version\":1,\"created_at_unix_ms\":1700000000000,\"completed_at_unix_ms\":1700003600000,\"completion_history_unix_ms\":[1700003600000]} -->\n\nHistorical body",
    )
    .unwrap();

    assert!(success(repo.path(), &["check"]).starts_with("OK:"));
    let before = json_success(repo.path(), &["list", "all", "--json"]);
    let legacy_json = json_task(&before, "t@lgcy");
    assert!(legacy_json["created_at"].is_null());
    assert!(legacy_json["completed_at"].is_null());
    assert!(legacy_json["completions"].as_array().unwrap().is_empty());
    let historical_json = json_task(&before, "t@hist");
    assert_eq!(historical_json["created_at"], "2023-11-14T22:13:20.000Z");
    assert_eq!(historical_json["completed_at"], "2023-11-14T23:13:20.000Z");
    assert_eq!(historical_json["completions"].as_array().unwrap().len(), 1);

    let migrated_legacy =
        PathBuf::from(success(repo.path(), &["set", "t@lgcy", "--priority", "5"]));
    let (legacy_metadata, legacy_body) = task_metadata_and_body(&migrated_legacy);
    assert_eq!(legacy_metadata["version"], 2);
    assert!(legacy_metadata["created_at"].is_null());
    assert!(
        legacy_metadata["completions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(legacy_body, "Legacy body\n");

    let migrated_historical =
        PathBuf::from(success(repo.path(), &["set", "t@hist", "--priority", "5"]));
    let (historical_metadata, historical_body) = task_metadata_and_body(&migrated_historical);
    assert_eq!(historical_metadata["version"], 2);
    assert_eq!(
        historical_metadata["completions"][0],
        "2023-11-14T23:13:20.000Z"
    );
    assert_eq!(historical_body, "Historical body");
}

#[test]
fn repeated_completions_preserve_the_original_body_and_append_timestamped_notes() {
    let repo = repository();
    let task = id_from_path(&created_path(
        repo.path(),
        &[
            "new",
            "Complete more than once",
            "--body",
            "Original intent",
        ],
    ));

    success(
        repo.path(),
        &["done", &task, "--completion-notes", "First outcome"],
    );
    success(repo.path(), &["reopen", &task]);
    let completed_path = PathBuf::from(success(
        repo.path(),
        &["done", &task, "--completion-notes", "Second outcome"],
    ));

    let (metadata, body) = task_metadata_and_body(&completed_path);
    let completions = metadata["completions"].as_array().unwrap();
    assert_eq!(completions.len(), 2);
    assert!(completions[0].as_str().unwrap() <= completions[1].as_str().unwrap());
    assert!(metadata.get("completed_at").is_none());
    assert!(body.starts_with("Original intent\n\n## Completion notes — "));
    assert_eq!(body.matches("## Completion notes — ").count(), 2);
    assert!(body.contains("First outcome"));
    assert!(body.contains("Second outcome"));
}

#[test]
fn list_json_is_versioned_resolved_and_explains_inherited_blocking() {
    let repo = repository();
    let independent_body =
        "# Context\n\nKeep café support & preserve Markdown.\n\n- First\n- Second";
    let independent = id_from_path(&created_path(
        repo.path(),
        &[
            "new",
            "Independent & Unicode café",
            "--priority",
            "0",
            "--body",
            independent_body,
        ],
    ));
    let parent = id_from_path(&created_path(
        repo.path(),
        &["new", "Parent work", "--priority", "2"],
    ));
    let child = id_from_path(&created_path(
        repo.path(),
        &[
            "new",
            "Child work",
            "--priority",
            "3.50",
            "--parent",
            &parent,
        ],
    ));
    let leaf = id_from_path(&created_path(
        repo.path(),
        &["new", "Leaf work", "--priority", "4", "--parent", &child],
    ));
    let parent_gate = id_from_path(&created_path(
        repo.path(),
        &[
            "new",
            "Parent approval gate",
            "--priority",
            "1",
            "--type",
            "gate",
            "--blocks",
            &parent,
        ],
    ));
    let child_gate = id_from_path(&created_path(
        repo.path(),
        &[
            "new",
            "Child approval gate",
            "--priority",
            "1.5",
            "--type",
            "gate",
            "--blocks",
            &child,
        ],
    ));
    let old_gate = id_from_path(&created_path(
        repo.path(),
        &[
            "new",
            "Completed old gate",
            "--priority",
            "9",
            "--type",
            "gate",
            "--blocks",
            &child,
        ],
    ));
    success(repo.path(), &["done", &old_gate]);

    let ready = json_success(repo.path(), &["list", "ready", "--json"]);
    assert_eq!(ready["schema_version"], 4);
    assert_eq!(ready["mode"], "ready");
    assert_eq!(
        ready["repository"],
        repo.path().file_name().unwrap().to_string_lossy().as_ref()
    );
    let ready_ids = ready["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(ready_ids.contains(&independent.as_str()));
    assert!(ready_ids.contains(&parent_gate.as_str()));
    assert!(ready_ids.contains(&child_gate.as_str()));
    assert!(!ready_ids.contains(&parent.as_str()));
    assert!(!ready_ids.contains(&child.as_str()));
    assert!(!ready_ids.contains(&leaf.as_str()));
    assert!(!ready_ids.contains(&old_gate.as_str()));
    assert_eq!(
        json_task(&ready, &independent)["body_markdown"],
        independent_body
    );
    assert!(
        json_task(&ready, &independent)["created_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert!(json_task(&ready, &independent)["completed_at"].is_null());
    let parent_gate_json = json_task(&ready, &parent_gate);
    assert_eq!(parent_gate_json["relationships"]["blocks"][0]["id"], parent);
    assert_eq!(
        parent_gate_json["relationships"]["blocks"][0]["priority_text"],
        "2"
    );
    assert_eq!(
        parent_gate_json["relationships"]["blocks"][0]["title"],
        "Parent work"
    );
    assert_eq!(
        json_success(repo.path(), &["list", "active", "--json"])["mode"],
        "ready"
    );

    let all = json_success(repo.path(), &["--color", "always", "list", "all", "--json"]);
    assert_eq!(all["mode"], "all");
    let tasks = all["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 7);
    for pair in tasks.windows(2) {
        let left_priority = pair[0]["priority"].as_f64().unwrap();
        let right_priority = pair[1]["priority"].as_f64().unwrap();
        assert!(
            left_priority < right_priority
                || (left_priority == right_priority
                    && pair[0]["id"].as_str() <= pair[1]["id"].as_str())
        );
    }

    let parent_json = json_task(&all, &parent);
    assert_eq!(parent_json["status"], "blocked");
    assert_eq!(parent_json["relationships"]["children"][0]["id"], child);
    assert_eq!(
        parent_json["relationships"]["blocked_by"][0]["id"],
        parent_gate
    );
    assert_eq!(parent_json["blocking_causes"][0]["kind"], "direct");
    assert_eq!(
        parent_json["blocking_causes"][0]["blocker"]["id"],
        parent_gate
    );

    let child_json = json_task(&all, &child);
    assert_eq!(child_json["priority_text"], "3.5");
    assert_eq!(child_json["relationships"]["parent"]["id"], parent);
    assert_eq!(child_json["relationships"]["children"][0]["id"], leaf);
    let declared_blockers = child_json["relationships"]["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reference| reference["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(declared_blockers.contains(&child_gate.as_str()));
    assert!(declared_blockers.contains(&old_gate.as_str()));
    assert_eq!(
        child_json["relationships"]["blocked_by"]
            .as_array()
            .unwrap()
            .iter()
            .find(|reference| reference["id"] == old_gate)
            .unwrap()["status"],
        "done"
    );
    let causes = child_json["blocking_causes"].as_array().unwrap();
    assert!(causes.iter().any(|cause| {
        cause["kind"] == "direct"
            && cause["blocker"]["id"] == child_gate
            && cause["blocked_target"]["id"] == child
    }));
    assert!(causes.iter().any(|cause| {
        cause["kind"] == "inherited"
            && cause["blocker"]["id"] == parent_gate
            && cause["blocked_target"]["id"] == parent
    }));
    assert_eq!(json_task(&all, &old_gate)["status"], "done");
    assert!(
        json_task(&all, &old_gate)["completed_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert_eq!(
        json_task(&all, &old_gate)["completions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        json_task(&all, &old_gate)["blocking_causes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn list_json_handles_empty_repositories_and_conflicting_output_flags() {
    let repo = repository();
    let empty = json_success(repo.path(), &["list", "all", "--json"]);
    assert_eq!(empty["schema_version"], 4);
    assert_eq!(empty["tasks"].as_array().unwrap().len(), 0);

    let (stdout, stderr) = failure(repo.path(), &["list", "--json", "--filenames"]);
    assert!(stdout.is_empty());
    assert!(stderr.contains("cannot be used with"));
    let (stdout, stderr) = failure(repo.path(), &["list", "--json", "--markdown"]);
    assert!(stdout.is_empty());
    assert!(stderr.contains("cannot be used with"));
}

#[test]
fn detects_cycles_before_listing_or_mutating_tasks() {
    let repo = repository();
    let tasks = repo.path().join("tasks");
    fs::create_dir_all(tasks.join("done")).unwrap();
    fs::write(
        tasks.join("t@aaaa, parent-t@bbbb, blocks-t@bbbb, p5, TASK, First.md"),
        "",
    )
    .unwrap();
    fs::write(tasks.join("t@bbbb, p5, TASK, Second.md"), "").unwrap();

    let (stdout, stderr) = failure(repo.path(), &["list"]);
    assert!(stdout.is_empty(), "cycle errors must not emit task output");
    assert!(stderr.contains("dependency cycle"));
    assert!(stderr.contains("t@aaaa"));
    assert!(stderr.contains("t@bbbb"));

    let (stdout, stderr) = failure(repo.path(), &["list", "all", "--json"]);
    assert!(stdout.is_empty(), "cycle errors must not emit partial JSON");
    assert!(stderr.contains("dependency cycle"));

    let (stdout, stderr) = failure(repo.path(), &["next"]);
    assert!(
        stdout.is_empty(),
        "cycle errors must not emit a next-task ID"
    );
    assert!(stderr.contains("dependency cycle"));

    let (stdout, stderr) = failure(repo.path(), &["done", "t@aaaa"]);
    assert!(stdout.is_empty());
    assert!(stderr.contains("dependency cycle"));
    assert!(
        tasks
            .join("t@aaaa, parent-t@bbbb, blocks-t@bbbb, p5, TASK, First.md")
            .is_file()
    );
}

#[test]
fn explains_prospective_dependency_cycles_before_changing_files() {
    let repo = repository();
    let first = id_from_path(&created_path(repo.path(), &["new", "First"]));
    let second = id_from_path(&created_path(
        repo.path(),
        &["new", "Second", "--blocks", &first],
    ));
    let before = success(repo.path(), &["list", "all", "--filenames"]);

    let (stdout, stderr) = failure(repo.path(), &["set", &first, "--blocks", &second]);
    assert!(stdout.is_empty());
    assert_layered_diagnostic(&stderr, "The requested change creates a dependency cycle.");
    assert!(stderr.contains("tat rejected the change"));
    assert!(stderr.contains("A cycle has no valid starting task"));
    assert!(stderr.contains("No task files were changed"));
    let mut cycle_ids = [&first, &second];
    cycle_ids.sort();
    assert!(stderr.contains(&format!(
        "{} -> {} -> {}",
        cycle_ids[0], cycle_ids[1], cycle_ids[0]
    )));
    assert!(stderr.contains("Run 'tatviewer'"));
    assert!(stderr.contains(&format!("tat set {second} --remove-blocks {first}")));
    assert!(stderr.contains(&format!("tat set {first} --blocks {second}")));
    assert_eq!(
        success(repo.path(), &["list", "all", "--filenames"]),
        before
    );
}

#[test]
fn refuses_to_remove_a_referenced_task() {
    let repo = repository();
    let parent = id_from_path(&created_path(repo.path(), &["new", "Parent"]));
    let child = id_from_path(&created_path(
        repo.path(),
        &["new", "Child", "--parent", &parent],
    ));
    let blocker = id_from_path(&created_path(
        repo.path(),
        &["new", "Blocker", "--blocks", &parent],
    ));

    let (_, error) = failure(repo.path(), &["remove", &parent, "--force"]);
    assert!(error.contains("still referenced"));
    assert!(error.contains(&child));
    assert!(error.contains(&format!("{blocker} names it in its blocks field")));

    let target = id_from_path(&created_path(repo.path(), &["new", "Second target"]));
    let referrer = id_from_path(&created_path(
        repo.path(),
        &["new", "Sole referrer", "--blocks", &target],
    ));
    let (_, error) = failure(repo.path(), &["remove", &target, "--force"]);
    assert!(error.contains(&format!("tat set {referrer} --remove-blocks {target}")));
    assert!(error.contains(&format!("tat remove {target} --force")));
}

#[test]
fn help_orients_new_users_and_solves_command_specific_questions() {
    let directory = support::tempdir();
    let top = success(directory.path(), &["-h"]);
    for expected in [
        "tat manages repository-local work as reviewable Markdown files. It is designed\nfor humans and agents that want a small, Git-friendly backlog without a service\nor database.",
        "QUICK START",
        "FIND THE RIGHT COMMAND",
        "LEARN AND TROUBLESHOOT",
        "tat help <COMMAND>    More detailed help for a particular command",
        "tat guide             Orientation and recommended workflow (How To)",
        "tat reference         The reference manual",
        "tat completions <SHELL>  Enable Bash or PowerShell Tab completion",
        "      --color <COLOR>  Control ANSI colors: [default: auto] [possible values: auto, always, never]",
        "  -h, --help           Print help",
        "  -V, --version        Print version",
    ] {
        assert!(
            top.contains(expected),
            "top-level help omitted {expected:?}"
        );
    }
    assert!(!top.contains("\nALIASES\n"));
    assert!(!top.contains("\nCOLOR\n"));
    assert!(!top.contains("Task IDs are generated"));

    let help_command = success(directory.path(), &["help"]);
    assert_eq!(help_command, top);
    let long_help_flag = success(directory.path(), &["--help"]);
    assert_eq!(long_help_flag, top);

    let output = tat(directory.path(), &[]);
    assert!(!output.status.success());
    let no_args_help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(no_args_help.contains("QUICK START"));
    assert!(no_args_help.contains("tat help <COMMAND>"));

    let (_, typo_error) = failure(directory.path(), &["lst"]);
    assert!(typo_error.contains("similar subcommand"));
    assert!(typo_error.contains("list"));

    let command_expectations = [
        ("new", "BODY INPUT"),
        ("list", "MODES"),
        ("next", "first ready task"),
        ("id", "No Git repository is required"),
        ("done", "not-done descendant"),
        ("set", "RELATIONSHIP OPTIONS"),
        ("reopen", "blocked again"),
        ("view", "--filename"),
        ("remove", "make other tasks ready"),
        ("check", "CHECKS"),
        ("guide", "HOW TO"),
        ("reference", "REFERENCE"),
    ];
    for (command, expected) in command_expectations {
        let help = success(directory.path(), &[command, "--help"]);
        assert!(help.contains("Usage:"), "{command} help omitted usage");
        assert!(
            help.contains("EXAMPLES") || matches!(command, "guide" | "reference"),
            "{command} help omitted examples"
        );
        assert!(
            help.contains(expected),
            "{command} help omitted {expected:?}"
        );
    }

    let help_subcommand = success(directory.path(), &["help", "set"]);
    assert!(help_subcommand.contains("RELATIONSHIP OPTIONS"));
    assert!(help_subcommand.contains("--clear-blocks"));

    let short_help = success(directory.path(), &["-h"]);
    let command_rows = [
        ("new", "Create a new task"),
        (
            "list",
            "List tasks, defaulting to tasks that are ready to execute [alias: ls]",
        ),
        ("next", "Print the ID of the first ready task"),
        ("id", "Extract a task ID from a task filename or path"),
        (
            "done",
            "Complete a not-done task [aliases: complete, close]",
        ),
        (
            "set",
            "Change a task while preserving its ID and storage location [aliases: edit, update, change]",
        ),
        ("reopen", "Reopen a done task [alias: open]"),
        ("view", "View one task [alias: show]"),
        (
            "remove",
            "Permanently remove an unreferenced task [aliases: delete, rm]",
        ),
        (
            "check",
            "Validate filenames, references, IDs, priorities, and dependency cycles [alias: validate]",
        ),
        ("guide", "Orientation and recommended workflow (How To)"),
        ("reference", "The reference manual"),
        (
            "completions",
            "Print a Bash or PowerShell completion script",
        ),
        (
            "help",
            "Print this message or the help of the given subcommand(s)",
        ),
    ];
    for (command, description) in command_rows {
        let expected = format!("  {command:<11}  {description}");
        assert!(
            short_help.lines().any(|line| line == expected),
            "compact -h omitted row {expected:?}\n{short_help}"
        );
    }
    assert!(!short_help.contains("Commands:\n  new\n"));
    assert!(!short_help.contains("\n  graph"));
}

#[test]
fn shell_completion_suggests_current_repository_tasks_and_preserves_static_generation() {
    let repo = repository();
    let work = id_from_path(&created_path(repo.path(), &["new", "Write the parser"]));
    let blocker = id_from_path(&created_path(
        repo.path(),
        &["new", "Approve the parser", "--blocks", &work],
    ));
    let finished = id_from_path(&created_path(repo.path(), &["new", "Finished task"]));
    success(repo.path(), &["done", &finished]);

    let bash = success(repo.path(), &["completions", "bash"]);
    assert!(bash.contains("complete -o default"));
    assert!(bash.contains("__complete"));
    let powershell = success(repo.path(), &["completions", "powershell"]);
    assert!(powershell.contains("Register-ArgumentCompleter -Native"));
    assert!(powershell.contains("__complete"));
    let static_bash = success(repo.path(), &["completions", "bash", "--static"]);
    assert!(static_bash.contains("complete -F"));
    assert!(!static_bash.contains("__complete"));
    let static_powershell = success(repo.path(), &["completions", "powershell", "--static"]);
    assert!(static_powershell.contains("Register-ArgumentCompleter"));
    assert!(!static_powershell.contains("__complete"));

    let commands = success(repo.path(), &["__complete", "--", "tat", "se"]);
    assert!(commands.contains("set\t"));
    let help_commands = success(repo.path(), &["__complete", "--", "tat", "help", "se"]);
    assert!(help_commands.contains("set\t"));
    let options = success(
        repo.path(),
        &["__complete", "--", "tat", "set", &work, "--t"],
    );
    assert!(options.contains("--type\t"));
    let trailing_options = success(repo.path(), &["__complete", "--", "tat", "view", &work, ""]);
    assert!(trailing_options.contains("--filename\t"));
    let types = success(
        repo.path(),
        &["__complete", "--", "tat", "set", &work, "--type", "f"],
    );
    assert!(types.contains("feature\t"));
    let modes = success(repo.path(), &["__complete", "--", "tat", "list", "b"]);
    assert!(modes.contains("blocked\t"));

    let done_ids = success(repo.path(), &["__complete", "--", "tat", "done", "t@"]);
    assert!(done_ids.contains(&format!("{work}\tWrite the parser · blocked · P5")));
    assert!(done_ids.contains(&blocker));
    assert!(!done_ids.contains(&finished));
    let reopen_ids = success(repo.path(), &["__complete", "--", "tat", "open", "t@"]);
    assert!(reopen_ids.contains(&finished));
    assert!(!reopen_ids.contains(&work));
    let parent_ids = success(
        repo.path(),
        &["__complete", "--", "tat", "set", &work, "--parent", "t@"],
    );
    assert!(parent_ids.contains(&blocker));
    let list_ids = success(
        repo.path(),
        &[
            "__complete",
            "--",
            "tat",
            "set",
            &work,
            "--blocks",
            &format!("{blocker},t@"),
        ],
    );
    assert!(list_ids.contains(&format!("{blocker},{work}\t")));
}

#[test]
fn colors_help_guides_tables_and_filenames_only_when_requested_for_captured_output() {
    let repo = repository();
    created_path(
        repo.path(),
        &[
            "new",
            "Colored task",
            "--priority",
            "2",
            "--type",
            "feature",
        ],
    );

    for args in [
        vec!["--color", "always", "--help"],
        vec!["--color", "always", "guide"],
        vec!["--color", "always", "reference"],
        vec!["--color", "always", "list"],
        vec!["list", "--filenames", "--color", "always"],
    ] {
        let output = tat(repo.path(), &args);
        assert!(output.status.success(), "{args:?} failed");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("\x1b["),
            "{args:?} did not produce ANSI styling"
        );
    }

    let colored_help = success(repo.path(), &["--color", "always", "--help"]);
    assert!(colored_help.contains("\x1b[1;36mUsage:"));
    assert!(colored_help.contains("  \x1b[1;32mnew\x1b[0m          Create a new task"));
    assert!(colored_help.contains("  \x1b[1;32mcheck\x1b[0m        Validate filenames"));
    let colored_reference = success(repo.path(), &["--color", "always", "reference"]);
    assert!(colored_reference.contains("\x1b[1;36mDEPENDENCIES AND READINESS\x1b[0m"));
    assert!(colored_reference.contains("\x1b[33mtat new \"Gate\" --blocks t@work\x1b[0m"));
    let colored_guide = success(repo.path(), &["--color", "always", "guide"]);
    assert!(!colored_guide.contains("\x1b[1;32m  tat manages"));
    let colored_automation = success(repo.path(), &["--color", "always", "reference"]);
    assert!(!colored_automation.contains("\x1b[1;32m  --body-file is supplied"));

    for args in [
        vec!["--help"],
        vec!["guide"],
        vec!["reference"],
        vec!["list"],
        vec!["list", "--filenames"],
        vec!["--color", "never", "--help"],
    ] {
        let output = tat(repo.path(), &args);
        assert!(output.status.success(), "{args:?} failed");
        assert!(
            !output.stdout.windows(2).any(|bytes| bytes == b"\x1b["),
            "{args:?} unexpectedly produced ANSI styling"
        );
    }

    let outside = support::tempdir();
    let output = tat(outside.path(), &["--color", "always", "check"]);
    assert!(!output.status.success());
    assert!(output.stderr.windows(2).any(|bytes| bytes == b"\x1b["));
    let output = tat(outside.path(), &["check", "--color", "always"]);
    assert!(!output.status.success());
    assert!(output.stderr.windows(2).any(|bytes| bytes == b"\x1b["));
    let output = tat(outside.path(), &["--color", "always", "done"]);
    assert!(!output.status.success());
    assert!(output.stderr.windows(2).any(|bytes| bytes == b"\x1b["));
}

#[test]
fn markdown_tables_align_every_column_border() {
    let repo = repository();
    created_path(repo.path(), &["new", "Short"]);
    created_path(
        repo.path(),
        &[
            "new",
            "A much longer task description",
            "--priority",
            "1.25",
            "--type",
            "feature",
        ],
    );

    let table = success(repo.path(), &["list", "all", "--color", "never"]);
    let lines = table.lines().collect::<Vec<_>>();
    assert!(lines.len() >= 4);
    assert!(lines[0].starts_with("|ID"));
    assert!(lines[0].contains("|Description"));
    assert!(lines[0].contains("|Prio|"));
    assert!(!lines[0].contains("| ID"));
    assert!(!lines[0].contains("Priority"));
    assert!(
        Regex::new(r"^\|[-]+\|[-]+\|[-]+:\|[-]+\|[-]+\|[-]+\|[-]+\|$")
            .unwrap()
            .is_match(lines[1])
    );
    assert!(lines[2..].iter().all(|line| !line.starts_with("| t@")));
    let expected = lines[0]
        .match_indices('|')
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    for line in lines {
        let actual = line
            .match_indices('|')
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "misaligned table row: {line}");
    }
}

#[test]
fn guide_and_reference_are_the_only_two_documentation_screens() {
    let directory = support::tempdir();
    let guide = success(directory.path(), &["guide"]);
    assert!(guide.starts_with("TAT GUIDE"));
    assert!(guide.contains("RECOMMENDED WORKFLOW"));
    for expected in [
        "WHEN TO USE TAT",
        "AGENT OPERATING RULES",
        "Ordinary implementation work does not by",
        "not silently absorb unrelated work",
        "required verification are finished",
        "deletion was explicitly requested",
        "WRITE ACTIONABLE TASKS",
        "useful negative controls",
        "Keep facts distinct from guesses",
        "review their repository diff",
    ] {
        assert!(guide.contains(expected), "guide omitted {expected:?}");
    }
    assert!(guide.contains("COMMON RECIPES"));
    assert!(guide.contains("--blocks $workId"));
    assert!(guide.contains("tat done $gateId"));
    assert!(!guide.contains("--blocks t@work"));
    assert!(!guide.contains("tat done t@gate"));
    assert!(
        guide.contains(
            "Task IDs are generated by `tat new`; do not invent or rename them manually."
        )
    );

    let reference = success(directory.path(), &["reference"]);
    for expected in [
        "TAT REFERENCE MANUAL",
        "COMMAND ALIASES",
        "COLOR",
        "TASK MODEL",
        "DEPENDENCIES AND READINESS",
        "AUTOMATION CONTRACT",
        "TROUBLESHOOTING",
        "schema_version: 4",
        "Markdown body",
        "created_at",
        "completions",
        "LastWriteTime",
        "blocking_causes",
        "No crash-consistency",
        "reads stdin to EOF",
        "Status values are ready, blocked, or done",
        "error: <terse expert summary>",
        "try: <specific recovery action>",
        "RECOVER AN ALREADY-INVALID STORE",
        "cannot perform the repair",
        "smallest manual rename",
    ] {
        assert!(
            reference.contains(expected),
            "reference omitted {expected:?}"
        );
    }
    assert!(!reference.contains("table State"));
    assert!(!reference.contains("Task IDs are generated by"));
    assert!(!reference.contains("@9"));
    assert!(reference.contains("required regression"));
    assert!(reference.contains("ask the policy owner"));

    let done_help = success(directory.path(), &["help", "done"]);
    assert!(done_help.contains("only after its work and required verification are finished"));
    assert!(done_help.contains("Completion notes are strongly recommended"));
    assert!(done_help.contains("--done-notes-file"));
    let remove_help = success(directory.path(), &["help", "remove"]);
    assert!(remove_help.contains("permanent deletion was explicitly requested"));

    for args in [["guide", "model"], ["reference", "model"]] {
        let (stdout, stderr) = failure(directory.path(), &args);
        assert!(stdout.is_empty());
        assert!(stderr.contains("unexpected argument 'model'"));
    }
}

#[test]
fn runtime_errors_lead_with_a_summary_then_explain_and_recover() {
    let directory = support::tempdir();
    let (stdout, stderr) = failure(directory.path(), &["check"]);
    assert!(stdout.is_empty());
    assert_layered_diagnostic(&stderr, "No Git repository was found.");
    assert!(stderr.contains("walks upward for a .git directory"));
    assert!(stderr.contains("not inside a Git repository"));
    assert!(stderr.contains("Change to the intended Git repository"));
}

#[test]
fn rejects_the_legacy_id_prefix_and_explains_the_t_at_prefix() {
    let repo = repository();
    success(repo.path(), &["new", "Initialize task store"]);

    let (stdout, stderr) = failure(repo.path(), &["view", "@9abcd"]);
    assert!(stdout.is_empty());
    assert!(stderr.contains("IDs must start with t@"));

    fs::write(
        repo.path()
            .join("tasks")
            .join("@9abcd, p5, TASK, Legacy identifier.md"),
        "",
    )
    .unwrap();
    let (stdout, stderr) = failure(repo.path(), &["check"]);
    assert!(stdout.is_empty());
    assert!(stderr.contains("task filename does not follow the required format"));
}
