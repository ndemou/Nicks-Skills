use clap::Parser;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tat::list_json::{TASK_LIST_SCHEMA_VERSION, TaskListDocument, TaskListItem, TaskReference};

static UNIQUE_OUTPUT_COUNTER: AtomicU64 = AtomicU64::new(0);

const HTML_TEMPLATE: &str = include_str!("../tatviewer.html");
const MERMAID_SCRIPT: &str = include_str!("../../assets/mermaid-11.16.0.min.js");
const MERMAID_LICENSE: &str = include_str!("../../assets/MERMAID_LICENSE.txt");

#[derive(Parser, Debug)]
#[command(
    name = "tatviewer",
    version,
    about = "Export an interactive, self-contained HTML dashboard for tat tasks.",
    long_about = "Export an interactive, self-contained HTML dashboard for tat tasks.\n\nThe default report starts with READY, BLOCKED, and RECENTLY DONE selected. Use --all to add OLD DONE cards; OLD DONE tasks are never included in the diagram. By default, recent means the 48-hour window ending at the repository's latest current completion. --recent accepts another number of hours or an inclusive oldest UTC calendar day. The collapsible Mermaid graph follows the task cards and can grow to its full natural height; the top-left graph button jumps to its stable #graph address. The Blocking flow view arranges dependencies left to right, with unrelated tasks listed separately. The Task families view shows the parent hierarchy without dependency arrows. Both views link to task cards. Truncated task details expand in place and render Markdown; relationship references provide delayed title-and-detail previews.",
    after_help = "EXAMPLES:\n  tatviewer\n  tatviewer --all\n  tatviewer --recent 72 --output-path .\\tasks.html\n  tatviewer --recent 2026-08-01 --open"
)]
struct Cli {
    /// Include OLD DONE in the initial report in addition to the default categories.
    #[arg(long)]
    all: bool,
    /// Recent window: hours before the latest completion, or the oldest included UTC day.
    #[arg(long, default_value = "48", value_name = "HOURS|YYYY-MM-DD")]
    recent: RecentWindow,
    /// Write to this path instead of a unique temporary HTML file.
    #[arg(long, value_name = "PATH", visible_alias = "output")]
    output_path: Option<PathBuf>,
    /// Open the exported dashboard in the default browser.
    #[arg(long)]
    open: bool,
    /// Replace an existing --output-path file.
    #[arg(long, requires = "output_path")]
    force: bool,
    /// Use a specific tat executable instead of the colocated binary.
    #[arg(long, value_name = "PATH", hide = true)]
    tat_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RecentWindow {
    Hours(u32),
    SinceDate(String),
}

impl RecentWindow {
    fn mode(&self) -> &'static str {
        match self {
            Self::Hours(_) => "hours",
            Self::SinceDate(_) => "date",
        }
    }

    fn value(&self) -> String {
        match self {
            Self::Hours(hours) => hours.to_string(),
            Self::SinceDate(date) => date.clone(),
        }
    }
}

impl std::str::FromStr for RecentWindow {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.bytes().all(|byte| byte.is_ascii_digit()) {
            let hours = value
                .parse::<u32>()
                .map_err(|_| "hours must fit in an unsigned 32-bit number".to_owned())?;
            if hours == 0 {
                return Err("hours must be greater than zero".to_owned());
            }
            return Ok(Self::Hours(hours));
        }
        if valid_calendar_date(value) {
            return Ok(Self::SinceDate(value.to_owned()));
        }
        Err("expected a positive number of hours or a valid date in YYYY-MM-DD form".to_owned())
    }
}

fn valid_calendar_date(value: &str) -> bool {
    if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100));
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 31,
    };
    (1..=days).contains(&day)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(path) => {
            println!("{}", path.display());
            if cli.open
                && let Err(error) = open_in_browser(&path)
            {
                eprintln!(
                    "error: dashboard was exported to '{}' but could not be opened: {error}",
                    path.display()
                );
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<PathBuf, String> {
    let caller_cwd = std::env::current_dir()
        .map_err(|error| format!("cannot read the current directory: {error}"))?;
    let tat_path = resolve_tat_path(cli.tat_path.as_deref())?;
    let document = load_all_tasks(&tat_path, &caller_cwd)?;
    validate_document(&document)?;
    let html = render_html(&document, cli.all, &cli.recent)?;
    write_dashboard(&html, cli.output_path.as_deref(), cli.force, &caller_cwd)
}

fn resolve_tat_path(override_path: Option<&Path>) -> Result<PathBuf, String> {
    let path = if let Some(path) = override_path {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| format!("cannot read the current directory: {error}"))?
                .join(path)
        }
    } else {
        let executable =
            std::env::current_exe().map_err(|error| format!("cannot locate tatviewer: {error}"))?;
        let directory = executable
            .parent()
            .ok_or_else(|| "cannot locate the directory containing tatviewer".to_owned())?;
        directory.join(format!("tat{}", std::env::consts::EXE_SUFFIX))
    };
    if !path.is_file() {
        return Err(format!(
            "the matching tat executable was not found at '{}'; keep tat and tatviewer together",
            path.display()
        ));
    }
    Ok(path)
}

fn load_all_tasks(tat_path: &Path, caller_cwd: &Path) -> Result<TaskListDocument, String> {
    let output = Command::new(tat_path)
        .args(["--color", "never", "list", "all", "--json"])
        .current_dir(caller_cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("cannot run '{}': {error}", tat_path.display()))?;
    if !output.status.success() {
        let status = output
            .status
            .code()
            .map_or_else(|| "terminated".to_owned(), |code| format!("exit {code}"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "tat could not load the repository ({status})\n\n{}",
            stderr.trim_end()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "tat returned invalid JSON: {error}; keep matching tat and tatviewer versions together"
        )
    })
}

fn validate_document(document: &TaskListDocument) -> Result<(), String> {
    if document.schema_version != TASK_LIST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported tat JSON schema {}; expected {}; keep matching tat and tatviewer versions together",
            document.schema_version, TASK_LIST_SCHEMA_VERSION
        ));
    }
    if document.mode != "all" {
        return Err(format!(
            "tat returned mode '{}' instead of 'all'; keep matching tat and tatviewer versions together",
            document.mode
        ));
    }
    let ids = document
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<HashSet<_>>();
    if ids.len() != document.tasks.len() {
        return Err("tat JSON contains duplicate task IDs".to_owned());
    }
    for task in &document.tasks {
        let relationships = &task.relationships;
        if let Some(parent) = &relationships.parent {
            validate_reference(&ids, &task.id, "parent", parent)?;
        }
        for reference in &relationships.children {
            validate_reference(&ids, &task.id, "child", reference)?;
        }
        for reference in &relationships.blocks {
            validate_reference(&ids, &task.id, "blocks", reference)?;
        }
        for reference in &relationships.blocked_by {
            validate_reference(&ids, &task.id, "blocked_by", reference)?;
        }
        for cause in &task.blocking_causes {
            validate_reference(&ids, &task.id, "blocking cause", &cause.blocker)?;
            validate_reference(
                &ids,
                &task.id,
                "blocking cause target",
                &cause.blocked_target,
            )?;
        }
    }
    Ok(())
}

fn validate_reference(
    ids: &HashSet<&str>,
    source: &str,
    relation: &str,
    reference: &TaskReference,
) -> Result<(), String> {
    if ids.contains(reference.id.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "tat JSON task '{source}' has a {relation} reference to missing task '{}'",
            reference.id
        ))
    }
}

fn mermaid_node_id(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn mermaid_label(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace(['\r', '\n'], " ")
}

fn mermaid_node(task: &TaskListItem, indentation: &str) -> String {
    let id = mermaid_node_id(&task.id);
    let label = mermaid_label(&format!("{}: {}", task.id, task.title));
    match task.task_type.as_str() {
        "GATE" => format!("{indentation}{id}{{\"{label}\"}}"),
        "BUG" => format!("{indentation}{id}{{{{\"{label}\"}}}}"),
        _ => format!("{indentation}{id}(\"{label}\")"),
    }
}

fn mermaid_status_class(status: &str) -> &'static str {
    match status {
        "ready" => "graphReady",
        "blocked" => "graphBlocked",
        _ => "graphDone",
    }
}

fn parse_canonical_utc_millis(value: &str) -> Result<i64, String> {
    let bytes = value.as_bytes();
    if !value.is_ascii()
        || bytes.len() != 24
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
        || bytes[23] != b'Z'
        || !valid_calendar_date(&value[..10])
    {
        return Err(format!(
            "'{value}' is not canonical UTC RFC 3339 with millisecond precision"
        ));
    }
    let parse = |range: std::ops::Range<usize>, name: &str| -> Result<u32, String> {
        value[range]
            .parse::<u32>()
            .map_err(|error| format!("invalid {name} in '{value}': {error}"))
    };
    let year = parse(0..4, "year")?;
    let month = parse(5..7, "month")?;
    let day = parse(8..10, "day")?;
    let hour = parse(11..13, "hour")?;
    let minute = parse(14..16, "minute")?;
    let second = parse(17..19, "second")?;
    let millisecond = parse(20..23, "millisecond")?;
    if hour > 23 || minute > 59 || second > 59 {
        return Err(format!("'{value}' contains an out-of-range time"));
    }
    let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    days.checked_mul(86_400_000)
        .and_then(|total| total.checked_add(i64::from(hour) * 3_600_000))
        .and_then(|total| total.checked_add(i64::from(minute) * 60_000))
        .and_then(|total| total.checked_add(i64::from(second) * 1_000))
        .and_then(|total| total.checked_add(i64::from(millisecond)))
        .ok_or_else(|| format!("'{value}' is outside the supported timestamp range"))
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let month_piece = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_piece + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn recent_completion_cutoff(
    document: &TaskListDocument,
    recent: &RecentWindow,
) -> Result<Option<i64>, String> {
    match recent {
        RecentWindow::SinceDate(date) => {
            parse_canonical_utc_millis(&format!("{date}T00:00:00.000Z")).map(Some)
        }
        RecentWindow::Hours(hours) => {
            let latest = document
                .tasks
                .iter()
                .filter_map(|task| task.completed_at.as_deref())
                .map(parse_canonical_utc_millis)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max();
            Ok(latest.map(|latest| latest.saturating_sub(i64::from(*hours) * 60 * 60 * 1_000)))
        }
    }
}

fn graph_task_is_visible(task: &TaskListItem, recent_cutoff: Option<i64>) -> Result<bool, String> {
    if task.status != "done" {
        return Ok(true);
    }
    let Some(completed_at) = task.completed_at.as_deref() else {
        return Ok(false);
    };
    let Some(recent_cutoff) = recent_cutoff else {
        return Ok(false);
    };
    Ok(parse_canonical_utc_millis(completed_at)? >= recent_cutoff)
}

fn build_mermaid_graph(
    document: &TaskListDocument,
    recent: &RecentWindow,
) -> Result<String, String> {
    let recent_cutoff = recent_completion_cutoff(document, recent)?;
    let mut tasks = document
        .tasks
        .iter()
        .filter_map(|task| match graph_task_is_visible(task, recent_cutoff) {
            Ok(true) => Some(Ok(task)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    tasks.sort_by(|left, right| left.id.cmp(&right.id));
    let visible_ids = tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<HashSet<_>>();
    let mut connected = HashSet::new();
    let mut edges = Vec::new();
    for task in &tasks {
        for target in &task.relationships.blocks {
            if visible_ids.contains(target.id.as_str()) {
                connected.insert(task.id.as_str());
                connected.insert(target.id.as_str());
                edges.push(format!(
                    "  {} --> {}",
                    mermaid_node_id(&task.id),
                    mermaid_node_id(&target.id)
                ));
            }
        }
    }
    // Isolated tasks are shown as a wrapping list, never as extra layout ranks.
    // Keep a valid node-only definition for exports without blocking links.
    if !connected.is_empty() {
        tasks.retain(|task| connected.contains(task.id.as_str()));
    }
    let mut lines = vec!["flowchart LR".to_owned()];
    lines.extend(tasks.iter().map(|task| mermaid_node(task, "  ")));
    lines.extend(edges);
    lines.extend([
        "  classDef graphReady fill:#ddf5e8,stroke:#137a47,color:#0b5935,stroke-width:2px"
            .to_owned(),
        "  classDef graphBlocked fill:#fee9e6,stroke:#b03a2e,color:#7f291f,stroke-width:2px"
            .to_owned(),
        "  classDef graphDone fill:#e9edf1,stroke:#66717f,color:#46505c,stroke-width:1.5px"
            .to_owned(),
    ]);
    for task in tasks {
        lines.push(format!(
            "  class {} {}",
            mermaid_node_id(&task.id),
            mermaid_status_class(&task.status)
        ));
    }
    Ok(lines.join("\n"))
}

fn render_html(
    document: &TaskListDocument,
    include_old_done: bool,
    recent: &RecentWindow,
) -> Result<String, String> {
    let json = serde_json::to_string(document)
        .map_err(|error| format!("cannot serialize dashboard task data: {error}"))?;
    let safe_json = escape_json_for_script(&json);
    let graph_json = serde_json::to_string(&build_mermaid_graph(document, recent)?)
        .map_err(|error| format!("cannot serialize dashboard task graph: {error}"))?;
    let safe_graph_json = escape_json_for_script(&graph_json);
    Ok(HTML_TEMPLATE
        .replace(
            "__TATVIEWER_INCLUDE_OLD_DONE__",
            if include_old_done { "true" } else { "false" },
        )
        .replace("__TATVIEWER_RECENT_MODE__", recent.mode())
        .replace("__TATVIEWER_RECENT_VALUE__", &recent.value())
        .replace("__TATVIEWER_MERMAID_LICENSE__", MERMAID_LICENSE)
        .replace("__TATVIEWER_MERMAID_JS__", MERMAID_SCRIPT)
        .replace("__TATVIEWER_GRAPH__", &safe_graph_json)
        .replace("__TATVIEWER_DATA__", &safe_json))
}

fn escape_json_for_script(json: &str) -> String {
    let mut safe = String::with_capacity(json.len());
    for character in json.chars() {
        match character {
            '<' => safe.push_str("\\u003c"),
            '>' => safe.push_str("\\u003e"),
            '&' => safe.push_str("\\u0026"),
            '\u{2028}' => safe.push_str("\\u2028"),
            '\u{2029}' => safe.push_str("\\u2029"),
            _ => safe.push(character),
        }
    }
    safe
}

fn write_dashboard(
    html: &str,
    output_path: Option<&Path>,
    force: bool,
    caller_cwd: &Path,
) -> Result<PathBuf, String> {
    match output_path {
        Some(path) => write_named_dashboard(html, path, force, caller_cwd),
        None => write_temporary_dashboard(html),
    }
}

fn write_temporary_dashboard(html: &str) -> Result<PathBuf, String> {
    let directory = std::env::temp_dir();
    let (mut file, path) = create_unique_file(&directory, "tatviewer-", ".html")?;
    if let Err(error) = write_complete(&mut file, html) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

fn write_named_dashboard(
    html: &str,
    requested: &Path,
    force: bool,
    caller_cwd: &Path,
) -> Result<PathBuf, String> {
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        caller_cwd.join(requested)
    };
    let parent = path
        .parent()
        .ok_or_else(|| format!("output path '{}' has no parent directory", path.display()))?;
    if !parent.is_dir() {
        return Err(format!(
            "output directory '{}' does not exist",
            parent.display()
        ));
    }
    let exists = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                return Err(format!("output path '{}' is a directory", path.display()));
            }
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "output path '{}' is a symbolic link and will not be replaced",
                    path.display()
                ));
            }
            if !force {
                return Err(format!(
                    "output file '{}' already exists; use --force to replace it",
                    path.display()
                ));
            }
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "cannot inspect output path '{}': {error}",
                path.display()
            ));
        }
    };
    if exists {
        let (mut temporary, temporary_path) = create_unique_file(parent, ".tatviewer-", ".tmp")?;
        if let Err(error) = write_complete(&mut temporary, html) {
            drop(temporary);
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
        drop(temporary);
        if let Err(error) = replace_file(&temporary_path, &path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!(
                "cannot replace output file '{}': {error}",
                path.display()
            ));
        }
    } else {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    format!(
                        "output file '{}' already exists; use --force to replace it",
                        path.display()
                    )
                } else {
                    format!("cannot create output file '{}': {error}", path.display())
                }
            })?;
        if let Err(error) = write_complete(&mut file, html) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(error);
        }
    }
    Ok(path)
}

fn create_unique_file(
    parent: &Path,
    prefix: &str,
    suffix: &str,
) -> Result<(File, PathBuf), String> {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_nanos();
    for attempt in 0..1000_u64 {
        let counter = UNIQUE_OUTPUT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!(
            "{prefix}{}-{seed:x}-{counter:x}-{attempt:x}{suffix}",
            std::process::id()
        );
        let path = parent.join(filename);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "cannot create a temporary file in '{}': {error}",
                    parent.display()
                ));
            }
        }
    }
    Err(format!(
        "cannot find a unique temporary filename in '{}'",
        parent.display()
    ))
}

fn write_complete(file: &mut File, html: &str) -> Result<(), String> {
    file.write_all(html.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot write the dashboard: {error}"))
}

#[cfg(target_os = "windows")]
fn replace_file(replacement: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut core::ffi::c_void,
            reserved: *mut core::ffi::c_void,
        ) -> i32;
    }

    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement_wide = replacement
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            replacement_wide.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_file(replacement: &Path, target: &Path) -> io::Result<()> {
    fs::rename(replacement, target)
}

#[cfg(target_os = "windows")]
fn open_in_browser(path: &Path) -> io::Result<()> {
    Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_in_browser(path: &Path) -> io::Result<()> {
    Command::new("open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_in_browser(path: &Path) -> io::Result<()> {
    Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
fn open_in_browser(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening a browser is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tat::list_json::{TaskListItem, TaskRelationships};

    fn document(title: &str, body_markdown: &str) -> TaskListDocument {
        TaskListDocument {
            schema_version: TASK_LIST_SCHEMA_VERSION,
            repository: "sample".to_owned(),
            mode: "all".to_owned(),
            tasks: vec![TaskListItem {
                id: "t@abcd".to_owned(),
                title: title.to_owned(),
                body_markdown: body_markdown.to_owned(),
                created_at: Some("2023-11-14T22:13:20.000Z".to_owned()),
                completed_at: None,
                completions: Vec::new(),
                priority: 2.5,
                priority_text: "2.5".to_owned(),
                task_type: "TASK".to_owned(),
                status: "ready".to_owned(),
                filename: "t@abcd, p2.5, TASK, Sample.md".to_owned(),
                relationships: TaskRelationships {
                    parent: None,
                    children: Vec::new(),
                    blocks: Vec::new(),
                    blocked_by: Vec::new(),
                },
                blocking_causes: Vec::new(),
            }],
        }
    }

    #[test]
    fn script_json_escaping_prevents_element_termination() {
        let escaped = escape_json_for_script("</script>&\u{2028}\u{2029}");
        assert_eq!(escaped, "\\u003c/script\\u003e\\u0026\\u2028\\u2029");
    }

    #[test]
    fn renderer_is_self_contained_and_replaces_every_placeholder() {
        let value = document("A & B </script>", "# Details\n\n<body>& text</body>");
        let graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(graph.contains("t_abcd(\"t@abcd: A &amp; B &lt;/script&gt;\")"));
        let html = render_html(&value, false, &RecentWindow::Hours(48)).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("data-include-old-done=\"false\""));
        assert!(html.contains("data-recent-mode=\"hours\""));
        assert!(html.contains("data-recent-value=\"48\""));
        assert!(html.contains("\\u003c/script\\u003e"));
        assert!(html.contains("const graphDefinition = \"flowchart LR"));
        assert!(html.contains("class t_abcd graphReady"));
        assert!(html.contains("boldGraphTaskId(graphElement, task)"));
        assert!(html.contains("const graphTasks = tasks.filter"));
        assert!(html.contains("id=\"graph-jump\""));
        assert!(html.contains("<details class=\"graph-panel\" id=\"graph\" open"));
        assert!(html.contains("animation: navigation-highlight 4.8s ease-out"));
        assert!(html.contains("background-color: #FFFF50"));
        assert!(html.contains("}, 4800);"));
        assert!(!html.contains("box-shadow: 0 0 0 4px color-mix"));
        assert!(html.contains("overflow-y: visible"));
        assert!(html.contains("max-height: none"));
        assert!(html.contains("wrappingWidth: 180"));
        assert!(!html.contains("setDashboardView(view, options = {})"));
        assert!(html.contains(".task-card[data-status=\"ready\"] .description-text"));
        assert!(html.contains("globalThis[\"mermaid\"]"));
        assert!(html.contains("The MIT License (MIT)"));
        assert!(!html.contains("__TATVIEWER_"));
        assert!(!html.contains("<script src="));
    }

    #[test]
    fn mermaid_graph_combines_type_shapes_with_status_classes() {
        let mut value = document("Sample", "Body");

        value.tasks[0].task_type = "GATE".to_owned();
        value.tasks[0].status = "ready".to_owned();
        let gate_graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(gate_graph.contains("t_abcd{\"t@abcd: Sample\"}"));
        assert!(gate_graph.contains("class t_abcd graphReady"));

        value.tasks[0].task_type = "BUG".to_owned();
        value.tasks[0].status = "blocked".to_owned();
        let bug_graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(bug_graph.contains("t_abcd{{\"t@abcd: Sample\"}}"));
        assert!(bug_graph.contains("class t_abcd graphBlocked"));

        value.tasks[0].task_type = "FEATURE".to_owned();
        value.tasks[0].status = "done".to_owned();
        value.tasks[0].completed_at = Some("2026-08-30T12:00:00.000Z".to_owned());
        let feature_graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(feature_graph.contains("t_abcd(\"t@abcd: Sample\")"));
        assert!(feature_graph.contains("class t_abcd graphDone"));
    }

    #[test]
    fn mermaid_graph_keeps_parent_relationships_out_of_the_blocking_layout() {
        let mut value = document("Parent", "Parent body");
        value.tasks[0].task_type = "FEATURE".to_owned();
        value.tasks[0].relationships.children.push(TaskReference {
            id: "t@chil".to_owned(),
            title: "Child".to_owned(),
            status: "blocked".to_owned(),
            priority_text: "4".to_owned(),
        });
        let mut child = value.tasks[0].clone();
        child.id = "t@chil".to_owned();
        child.title = "Child".to_owned();
        child.status = "blocked".to_owned();
        child.task_type = "TASK".to_owned();
        child.filename = "t@chil, parent-t@abcd, p4, TASK, Child.md".to_owned();
        child.relationships.parent = Some(TaskReference {
            id: "t@abcd".to_owned(),
            title: "Parent".to_owned(),
            status: "ready".to_owned(),
            priority_text: "2.5".to_owned(),
        });
        child.relationships.children.clear();
        value.tasks.push(child);

        let graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(graph.contains("t_abcd(\"t@abcd: Parent\")"));
        assert!(graph.contains("t_chil(\"t@chil: Child\")"));
        assert!(!graph.contains("subgraph"));
        assert!(!graph.contains(" --> "));
        let html = render_html(&value, false, &RecentWindow::Hours(48)).unwrap();
        assert!(html.contains("id=\"graph-families\""));
        assert!(html.contains("renderGraphFamilies()"));
    }

    #[test]
    fn mermaid_graph_never_includes_old_done_tasks() {
        let mut value = document("Active", "Body");
        let mut recent_done = value.tasks[0].clone();
        recent_done.id = "t@newd".to_owned();
        recent_done.title = "Recently done".to_owned();
        recent_done.status = "done".to_owned();
        recent_done.completed_at = Some("2026-08-30T12:00:00.000Z".to_owned());
        let mut old_done = recent_done.clone();
        old_done.id = "t@oldd".to_owned();
        old_done.title = "Old done".to_owned();
        old_done.completed_at = Some("2026-08-27T11:59:59.999Z".to_owned());
        value.tasks.extend([recent_done, old_done]);

        let graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(graph.contains("t_newd(\"t@newd: Recently done\")"));
        assert!(!graph.contains("t_oldd"));
        assert!(!graph.contains("Old done"));
    }

    #[test]
    fn access_workflow_preserves_cross_family_edges_without_layout_containers() {
        let value: TaskListDocument =
            serde_json::from_str(include_str!("../../tests/fixtures/access-workflow.json"))
                .unwrap();
        assert_eq!(value.tasks.len(), 17);
        let graph = build_mermaid_graph(&value, &RecentWindow::Hours(48)).unwrap();
        assert!(graph.starts_with("flowchart LR\n"));
        assert!(!graph.contains("subgraph"));
        let expected = value
            .tasks
            .iter()
            .flat_map(|task| {
                task.relationships.blocks.iter().map(|target| {
                    format!(
                        "  {} --> {}",
                        mermaid_node_id(&task.id),
                        mermaid_node_id(&target.id)
                    )
                })
            })
            .collect::<HashSet<_>>();
        let actual = graph
            .lines()
            .filter(|line| line.contains(" --> "))
            .map(str::to_owned)
            .collect::<HashSet<_>>();
        assert_eq!(expected.len(), 12);
        assert_eq!(actual, expected);
        // Family-only parents and unconnected tasks cannot enlarge the flow.
        for id in ["t_m46p", "t_goz4", "t_ypov", "t_pyph", "t_2cio", "t_q3b3"] {
            assert!(!graph.contains(id), "isolated task {id} entered the flow");
        }
        let html = render_html(&value, false, &RecentWindow::Hours(48)).unwrap();
        let start = html
            .find("<script id=\"task-data\" type=\"application/json\">")
            .unwrap()
            + "<script id=\"task-data\" type=\"application/json\">".len();
        let end = start + html[start..].find("</script>").unwrap();
        let exported: serde_json::Value = serde_json::from_str(&html[start..end]).unwrap();
        // Every task and parent relationship remains available in Task families.
        assert_eq!(exported, serde_json::to_value(&value).unwrap());
    }

    #[test]
    fn recent_window_accepts_positive_hours_and_valid_calendar_dates() {
        assert_eq!(
            "72".parse::<RecentWindow>().unwrap(),
            RecentWindow::Hours(72)
        );
        assert_eq!(
            "2024-02-29".parse::<RecentWindow>().unwrap(),
            RecentWindow::SinceDate("2024-02-29".to_owned())
        );
        for invalid in ["0", "-1", "1.5", "2023-02-29", "2026-13-01", "today"] {
            assert!(
                invalid.parse::<RecentWindow>().is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn document_validation_rejects_missing_relationship_targets() {
        let mut value = document("Sample", "Body");
        value.tasks[0].relationships.children.push(TaskReference {
            id: "t@lost".to_owned(),
            title: "Missing".to_owned(),
            status: "done".to_owned(),
            priority_text: "5".to_owned(),
        });
        let error = validate_document(&value).unwrap_err();
        assert!(error.contains("missing task 't@lost'"));
    }
}
