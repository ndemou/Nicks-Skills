mod support;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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

fn tat_success(repository: &Path, args: &[&str]) -> String {
    let output = tat(repository, args);
    assert!(
        output.status.success(),
        "tat {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 tat stdout")
        .trim()
        .to_owned()
}

fn created_id(repository: &Path, args: &[&str]) -> String {
    let path = PathBuf::from(tat_success(repository, args));
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .to_owned()
}

fn viewer(repository: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tatviewer"))
        .arg("--tat-path")
        .arg(env!("CARGO_BIN_EXE_tat"))
        .args(args)
        .current_dir(repository)
        .output()
        .expect("run tatviewer")
}

fn embedded_json(html: &str) -> Value {
    let opening = "<script id=\"task-data\" type=\"application/json\">";
    let start = html.find(opening).expect("task-data opening tag") + opening.len();
    let end = html[start..]
        .find("</script>")
        .map(|offset| start + offset)
        .expect("task-data closing tag");
    serde_json::from_str(&html[start..end]).expect("valid embedded task JSON")
}

fn embedded_graph(html: &str) -> String {
    let opening = "const graphDefinition = ";
    let start = html.find(opening).expect("graph definition") + opening.len();
    let end = html[start..]
        .find(';')
        .map(|offset| start + offset)
        .expect("graph definition terminator");
    serde_json::from_str(&html[start..end]).expect("valid embedded graph JSON string")
}

#[test]
fn exports_a_self_contained_dashboard_with_issue_tree_and_blocking_graph() {
    let repo = repository();
    let parent = created_id(
        repo.path(),
        &["new", "Parent task", "--priority", "2", "--type", "feature"],
    );
    let child_body = "# Goal\n\nShow the complete issue body.\n\n- Preserve line breaks\n- Keep <script>alert('safe')</script> inert\n- Include every final line";
    let child = created_id(
        repo.path(),
        &[
            "new",
            "Child task with a deliberately long title that wraps across several visual lines in the dashboard",
            "--parent",
            &parent,
            "--body",
            child_body,
        ],
    );
    let gate = created_id(
        repo.path(),
        &[
            "new",
            "Approval gate",
            "--priority",
            "1",
            "--type",
            "gate",
            "--blocks",
            &child,
        ],
    );
    let nested = repo.path().join("nested directory");
    fs::create_dir(&nested).unwrap();
    let relative_output = Path::new("task view & report.html");
    let output = viewer(
        &nested,
        &["--output-path", relative_output.to_str().unwrap()],
    );
    assert!(
        output.status.success(),
        "tatviewer failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected_path = nested.join(relative_output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        expected_path.to_string_lossy()
    );
    let html = fs::read_to_string(&expected_path).expect("dashboard HTML");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<body>"));
    assert!(html.contains("data-include-old-done=\"false\""));
    assert!(html.contains("data-recent-mode=\"hours\""));
    assert!(html.contains("data-recent-value=\"48\""));
    assert!(!html.contains("data-generated-ms="));
    assert!(html.contains("Content-Security-Policy"));
    assert!(html.contains("<p class=\"eyebrow\">tat dashboard</p>"));
    assert!(!html.contains("tat task dashboard"));
    assert!(html.contains("class=\"result-count header-result-count\" id=\"result-count\""));
    assert!(html.contains("class=\"header-toolbar\""));
    assert!(html.contains(".header-toolbar {\n      display: flex;\n      flex-wrap: wrap;"));
    assert!(html.contains(
        ".summary,\n    .type-summary {\n      display: flex;\n      flex: 0 1 auto;\n      flex-wrap: wrap;"
    ));
    assert!(html.contains("class=\"header-separator\" aria-hidden=\"true\">|</span>"));
    assert!(html.contains("#repository-name {"));
    assert!(html.contains("font-size: 1rem;\n      font-weight: 650;"));
    assert!(html.contains("--font-ui: \"Segoe UI Variable Text\", Aptos"));
    assert!(html.contains("--font-display: \"Segoe UI Variable Display\""));
    assert!(html.contains("--font-mono: \"Cascadia Mono\""));
    assert!(html.contains("font-family: var(--font-ui)"));
    assert!(html.contains("font-optical-sizing: auto"));
    assert!(html.contains("--text: #202a35"));
    assert!(html.contains("--accent: #2468b2"));
    assert!(html.contains("transform: rotate(180deg)"));
    assert!(html.contains(".summary-button,\n    .type-filter-button,\n    .clear-button"));
    assert!(html.contains("flex: 0 0 2rem"));
    assert!(html.contains(".search-control:focus-within"));
    assert!(html.contains(".search-control:has(input:not(:placeholder-shown))"));
    assert!(html.contains("class=\"reset-icon\""));
    assert!(html.contains("stroke=\"currentColor\""));
    assert!(!html.contains("🔄"));
    assert!(!html.contains("class=\"heading-row\""));
    assert!(!html.contains("class=\"filter-stack\""));
    assert!(!html.contains("class=\"controls\""));
    let toolbar_end = html
        .find("</div>\n      <p class=\"result-count header-result-count\"")
        .unwrap();
    assert!(html.find("class=\"header-toolbar\"").unwrap() < toolbar_end);
    assert!(html.find("id=\"repository-name\"").unwrap() < toolbar_end);
    assert!(html.find("data-status-filter=\"ready\"").unwrap() < toolbar_end);
    assert!(html.find("data-type-filter=\"BUG\"").unwrap() < toolbar_end);
    assert!(html.find("id=\"search\"").unwrap() < toolbar_end);
    assert!(html.find("id=\"clear-filters\"").unwrap() < toolbar_end);
    assert!(!html.contains("snapshot-meta"));
    assert!(html.contains("aria-label=\"Search tasks\""));
    assert!(!html.contains("aria-label=\"Sort tasks\""));
    assert!(html.contains("class=\"control-icon\""));
    assert!(html.contains("id=\"task-graph\""));
    assert!(html.contains("id=\"graph-jump\""));
    assert!(html.contains("aria-label=\"Jump to dependency graph\""));
    assert!(html.contains("aria-controls=\"graph\""));
    assert!(html.contains("<details class=\"graph-panel\" id=\"graph\" open"));
    assert!(html.contains("elements.graphPanel.open = true"));
    assert!(html.contains("history.pushState(null, \"\", \"#graph\")"));
    assert!(html.contains("window.location.hash === \"#graph\""));
    assert!(html.contains("focusGraph(false)"));
    assert!(html.contains("animation: navigation-highlight 2.88s ease-out"));
    assert!(html.contains("background-color: #FFFF50"));
    assert!(html.contains("}, 2880);"));
    assert!(html.contains("--ready-text: color-mix(in srgb, var(--ready) 70%, black)"));
    assert!(html.contains("--blocked-text: color-mix(in srgb, var(--blocked) 70%, black)"));
    assert!(html.contains("--ready-rail: #42965E"));
    assert!(html.contains("--blocked-rail: #9E453F"));
    assert!(html.contains("border-left-color: var(--ready-rail)"));
    assert!(html.contains("border-left-color: var(--blocked-rail)"));
    assert!(html.contains("--type-bug: #8F4512"));
    assert!(html.contains("--type-feature: #5B4796"));
    assert!(html.contains("--type-task: #1F6483"));
    assert!(html.contains(".badge-type-bug"));
    assert!(html.contains(".badge-type-feature"));
    assert!(html.contains(".badge-type-task"));
    assert!(html.contains("`badge-type-${task.type.toLowerCase()}`"));
    assert!(!html.contains("box-shadow: 0 0 0 4px color-mix"));
    assert!(html.contains("overflow-y: visible"));
    assert!(html.contains("max-height: none"));
    assert!(html.contains("nodeSpacing: 26"));
    assert!(html.contains("rankSpacing: 54"));
    assert!(
        html.find("id=\"task-list\"").unwrap()
            < html
                .find("<details class=\"graph-panel\" id=\"graph\"")
                .unwrap()
    );
    assert!(!html.contains("id=\"view-toggle\""));
    assert!(!html.contains("id=\"graph-workspace\""));
    assert!(!html.contains("id=\"graph-families\""));
    assert!(!html.contains("graph-families-button"));
    assert!(!html.contains("renderGraphFamilies"));
    assert!(html.contains("renderTaskGraph"));
    assert!(html.contains("task-graph-link"));
    assert!(html.contains("boldGraphTaskId(graphElement, task)"));
    assert!(html.contains("findGraphTaskElement"));
    assert!(html.contains("arrows show BLOCKS, left to right"));
    assert!(html.contains("class=\"graph-key\""));
    assert!(!html.contains(".task-card[data-status=\"ready\"] .description-text"));
    assert!(!html.contains(".task-card[data-status=\"blocked\"] .description-text"));
    assert!(!html.contains(".task-card[data-status=\"done\"] .description-text"));
    assert!(html.contains(".ref-title-ready {\n      color: var(--ready-text);"));
    assert!(html.contains(".ref-title-blocked {\n      color: var(--blocked-text);"));
    assert!(html.contains("border-radius: 8px;\n      background: var(--surface-muted);"));
    assert!(html.contains("font-size: clamp(1rem, 1.5vw, 1.16rem);\n      font-weight: 600;"));
    assert!(html.contains("font-size: 0.86rem;\n      line-height: 1.48;"));
    assert!(html.contains("font-size: 0.87rem;\n      line-height: 1.52;"));
    assert!(html.contains("globalThis[\"mermaid\"]"));
    assert!(html.contains("The MIT License (MIT)"));
    assert!(html.contains("createTaskReference"));
    assert!(html.contains("copyTaskId"));
    assert!(html.contains("navigateToTask"));
    assert!(html.contains("data-status-filter=\"ready\""));
    assert!(html.contains("data-status-filter=\"blocked\""));
    assert!(html.contains("data-status-filter=\"recent-done\""));
    assert!(html.contains("data-status-filter=\"old-done\""));
    assert!(html.contains("data-type-filter=\"BUG\""));
    assert!(html.contains("data-type-filter=\"FEATURE\""));
    assert!(html.contains("data-type-filter=\"TASK\""));
    assert!(html.contains("data-type-filter=\"GATE\""));
    assert!(!html.contains("id=\"type-filter\""));
    assert!(!html.contains("id=\"sort\""));
    assert!(!html.contains("compareTasks"));
    assert!(!html.contains("directReferences"));
    assert!(html.contains("function taskSearchText(task)"));
    assert!(html.contains("createTreeView"));
    assert!(html.contains("task-card-context"));
    assert!(html.contains("shown for context"));
    assert!(html.contains("compact ? task.status.charAt(0).toUpperCase() : task.status"));
    assert!(html.contains("makeElement(\"div\", \"card-headline\")"));
    assert!(html.contains(".card-headline .description-toggle"));
    assert!(html.contains("Click for full details"));
    assert!(!html.contains("Show full details"));
    assert!(html.contains("createDetails"));
    assert!(html.contains("createBlockedByRelationship"));
    assert!(html.contains("createCompactRelationships"));
    assert!(html.contains("createTaskTreeItems"));
    assert!(html.contains(".task-tree-children"));
    assert!(html.contains("border-left: 2px solid #545F6B"));
    assert!(html.contains("border-top: 2px solid #545F6B"));
    assert!(html.contains(".task-tree-parent-link"));
    assert!(html.contains("left: calc(-1 * var(--tree-indent) - 4px)"));
    assert!(html.contains("width: 10px"));
    assert!(html.contains("navigateToTask(parentTask.id, true)"));
    assert!(html.contains(".card-status-rail"));
    assert!(html.contains("writing-mode: vertical-rl"));
    assert!(html.contains("createCardStatusRail"));
    assert!(html.contains("colorTitleByStatus: true"));
    assert!(!html.contains("[\"CHILDREN\", relationships.children || []]"));
    assert!(html.contains("return createBadge(`${label}:`, \"badge-blocked\")"));
    assert!(html.contains("hideBlockedStatus: label === \"BLOCKS\""));
    assert!(!html.contains("createParentRelationship"));
    assert!(html.contains("reference.status !== \"done\""));
    assert!(html.contains("isVisuallyTruncated"));
    assert!(html.contains("showPreview: false"));
    assert!(!html.contains("attachHoverPreview(button, task, \"title\")"));
    assert!(html.contains("renderMarkdown"));
    assert!(html.contains("renderMarkdownPreview"));
    assert!(html.contains("markdown-line-break"));
    assert!(html.contains("\"↵ \""));
    assert!(!html.contains("<script>alert('safe')</script>"));
    assert!(!html.contains("<script src="));

    let graph = embedded_graph(&html);
    assert!(graph.starts_with("flowchart LR\n"));
    assert!(!graph.contains("subgraph"));
    assert!(graph.contains(&format!("{}(\"{child}:", child.replace('@', "_"))));
    assert!(graph.contains(&format!("{}{{\"{gate}:", gate.replace('@', "_"))));
    assert!(!graph.contains("-->|parent|"));
    assert!(graph.contains(" --> "));
    assert!(!graph.contains(&parent.replace('@', "_")));
    assert!(graph.contains(&format!("class {} graphBlocked", child.replace('@', "_"))));

    let data = embedded_json(&html);
    assert_eq!(data["mode"], "all");
    assert_eq!(data["tasks"].as_array().unwrap().len(), 3);
    for id in [&parent, &child, &gate] {
        assert!(
            data["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| task["id"] == *id)
        );
    }
    let child_data = data["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == child)
        .unwrap();
    assert_eq!(child_data["body_markdown"], child_body);
    assert!(child_data["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(child_data["completed_at"].is_null());
    assert_eq!(child_data["status"], "blocked");
    assert_eq!(child_data["relationships"]["parent"]["id"], parent);
    assert_eq!(child_data["relationships"]["parent"]["priority_text"], "2");
    assert_eq!(child_data["relationships"]["blocked_by"][0]["id"], gate);
    assert_eq!(
        child_data["relationships"]["blocked_by"][0]["priority_text"],
        "1"
    );
}

#[test]
fn protects_existing_outputs_unless_force_is_explicit() {
    let repo = repository();
    created_id(repo.path(), &["new", "Task for overwrite test"]);
    let output_path = repo.path().join("dashboard.html");
    fs::write(&output_path, "keep me").unwrap();

    let output = viewer(
        repo.path(),
        &["--output-path", output_path.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
    assert_eq!(fs::read_to_string(&output_path).unwrap(), "keep me");

    let output = viewer(
        repo.path(),
        &[
            "--all",
            "--recent",
            "72",
            "--output-path",
            output_path.to_str().unwrap(),
            "--force",
        ],
    );
    assert!(
        output.status.success(),
        "forced export failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = fs::read_to_string(&output_path).unwrap();
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("data-include-old-done=\"true\""));
    assert!(html.contains("data-recent-mode=\"hours\""));
    assert!(html.contains("data-recent-value=\"72\""));
}

#[test]
fn recent_window_accepts_an_oldest_utc_day_and_rejects_invalid_values() {
    let repo = repository();
    created_id(repo.path(), &["new", "Task for recent-window test"]);
    let output_path = repo.path().join("dated-dashboard.html");
    let output = viewer(
        repo.path(),
        &[
            "--recent",
            "2024-02-29",
            "--output-path",
            output_path.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    let html = fs::read_to_string(output_path).unwrap();
    assert!(html.contains("data-recent-mode=\"date\""));
    assert!(html.contains("data-recent-value=\"2024-02-29\""));

    for invalid in ["0", "2023-02-29", "yesterday"] {
        let output = viewer(repo.path(), &["--recent", invalid]);
        assert!(!output.status.success(), "accepted {invalid}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid value"));
    }
}

#[test]
fn all_report_keeps_old_done_cards_but_excludes_them_from_the_graph() {
    let repo = repository();
    created_id(repo.path(), &["new", "Current graph task"]);
    let old_done = created_id(repo.path(), &["new", "Old done card only", "--type", "bug"]);
    tat_success(repo.path(), &["done", &old_done]);
    let output_path = repo.path().join("all-dashboard.html");
    let output = viewer(
        repo.path(),
        &[
            "--all",
            "--recent",
            "9999-12-31",
            "--output-path",
            output_path.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "all export failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = fs::read_to_string(output_path).unwrap();
    let data = embedded_json(&html);
    assert!(
        data["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["id"] == old_done)
    );
    let graph = embedded_graph(&html);
    assert!(!graph.contains(&old_done.replace('@', "_")));
    assert!(!graph.contains("Old done card only"));
}

#[test]
fn failed_repository_loading_creates_no_dashboard() {
    let outside = support::tempdir();
    let output_path = outside.path().join("must-not-exist.html");
    let output = viewer(
        outside.path(),
        &["--output-path", output_path.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(!output_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No Git repository was found"));
}

#[test]
fn default_output_is_unique_absolute_and_persistent() {
    let repo = repository();
    created_id(repo.path(), &["new", "Temporary dashboard task"]);
    let first = viewer(repo.path(), &[]);
    let second = viewer(repo.path(), &[]);
    assert!(first.status.success());
    assert!(second.status.success());
    let first_path = PathBuf::from(String::from_utf8(first.stdout).unwrap().trim());
    let second_path = PathBuf::from(String::from_utf8(second.stdout).unwrap().trim());
    assert!(first_path.is_absolute());
    assert!(second_path.is_absolute());
    assert_ne!(first_path, second_path);
    assert!(first_path.is_file());
    assert!(second_path.is_file());
    fs::remove_file(first_path).unwrap();
    fs::remove_file(second_path).unwrap();
}
