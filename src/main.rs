use std::{collections::HashMap, fs::File, io::Write, process::Command};

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

// regex to match git log output
static RGX_GIT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<date>\d{4}-\d{2}-\d{2})(?P<parens>.*)(?P<tag>docs|feat|fix|refactor|style): (?P<text>.*)").unwrap()
});

// regex to match git tags
static RGX_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"tag: (?P<version>[v0-9.]+)").unwrap());

// extract git log output as lines
fn git_log() -> Vec<String> {
    let output = Command::new("git")
        .args(["log", "--pretty=%cs %d %s"])
        .output()
        .expect("`git` must be installed");

    if !output.status.success() {
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        panic!("{}", stderr);
    }

    std::str::from_utf8(&output.stdout)
        .unwrap()
        .split('\n')
        .map(|s| s.to_string())
        .rev()
        .collect::<Vec<String>>()
}

// extract the remote origin to generate comparison and release urls
fn git_remote_url() -> String {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .expect("`git` must be installed");

    if !output.status.success() {
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        panic!("{}", stderr);
    }

    let txt = std::str::from_utf8(&output.stdout).unwrap().to_string();

    let rgx = Regex::new(".git\n$").unwrap();
    rgx.replace(&txt, "").to_string()
}

// extract regex matches as strings
fn get_match(caps: &Option<Captures>, kind: &str) -> Option<String> {
    caps.as_ref()
        .and_then(|c| c.name(kind))
        .map(|m| String::from(m.as_str()))
}

// create changelog header for each version
fn get_header(version: &Option<String>, version_next: &str, url: &str, date: &str) -> Vec<String> {
    let header;
    let version_text = match version_next {
        "main" => "Unreleased",
        _ => version_next,
    };
    if let Some(ref version_prev) = version {
        header = format!(
            "## [{}]({}/compare/{}...{}) - {}",
            version_text, url, version_prev, version_next, date
        );
    } else {
        header = format!(
            "## [{}]({}/releases/tag/{}) - {}",
            version_text, url, version_next, date
        );
    }
    vec![header, "".to_string()]
}

// capitalize first letter and format bulletpoint
fn get_list_bullet(s: &str) -> String {
    let mut c = s.chars();
    let bullet = match c.next() {
        None => String::new(),
        Some(ch) => ch.to_uppercase().collect::<String>() + c.as_str(),
    };
    format!("* {}", bullet)
}

// create specific changelog chunk for each version
fn get_chunk(chunks: &mut HashMap<String, Vec<String>>, tag: &str, header: &str) -> Vec<String> {
    let mut chunk = Vec::new();
    if let Some(items) = chunks.get_mut(tag) {
        if !items.is_empty() {
            chunk.append(&mut vec![format!("### {}", header), "".to_string()]);
            for added in items.clone().iter().rev() {
                chunk.push(get_list_bullet(added));
            }
            chunk.push("".to_string());
            items.clear();
        }
    }
    chunk
}

// create all changelog chunks for each version
fn get_all_chunks(chunks: &mut HashMap<String, Vec<String>>) -> Vec<String> {
    let mut chunk = Vec::new();
    chunk.append(&mut get_chunk(chunks, "feat", "Added"));
    chunk.append(&mut get_chunk(chunks, "refactor", "Changed"));
    chunk.append(&mut get_chunk(chunks, "fix", "Fixed"));
    chunk
}

// check if changelog chunk already exists
fn has_chunk(chunks: &HashMap<String, Vec<String>>, tag: &str) -> bool {
    chunks.get(tag).map(|v| !v.is_empty()).unwrap_or(false)
}

// check if any changelog chunks exist
fn any_chunks(chunks: &HashMap<String, Vec<String>>) -> bool {
    let v = vec![
        has_chunk(chunks, "feat"),
        has_chunk(chunks, "refactor"),
        has_chunk(chunks, "fix"),
    ];
    v.iter().any(|v| *v)
}

fn add_chunks(
    caps: &Option<Captures>,
    chunks: &mut HashMap<String, Vec<String>>,
    version: &Option<String>,
    version_next: &str,
    url: &str,
) {
    // if version 1.0.0 has no entry, add a default one
    if (version_next == "v1.0.0" || version_next == "1.0.0") && !any_chunks(chunks) {
        chunks
            .entry("feat".to_string())
            .or_insert(Vec::new())
            .push("initial release".to_string());
    }
    // append changelog chunks if they exist
    if any_chunks(chunks) {
        let date = get_match(caps, "date").unwrap();
        let mut chunk = Vec::new();
        chunk.append(&mut get_header(version, version_next, url, &date));
        chunk.append(&mut get_all_chunks(chunks));
        let chunk = chunk.join("\n");
        chunks
            .entry("final".to_string())
            .or_insert(Vec::new())
            .push(chunk);
    }
}

// create changelog from version chunks
fn get_changelog(chunks: &HashMap<String, Vec<String>>) -> String {
    let mut changelog = vec!["# Changelog".to_string(), "".to_string()];
    changelog.append(
        &mut chunks["final"]
            .iter()
            .rev()
            .map(|s| s.to_string())
            .collect::<Vec<String>>(),
    );
    changelog.join("\n").trim_end().to_string()
}

fn main() {
    let url = git_remote_url();
    let log = git_log();

    let mut chunks: HashMap<String, Vec<String>> = HashMap::new();
    let mut version: Option<String> = None;

    let last = log.len() - 1;
    for (i, line) in log.iter().enumerate() {
        let caps = RGX_GIT.captures(line);
        if let Some(tag) = get_match(&caps, "tag") {
            if let Some(text) = get_match(&caps, "text") {
                chunks.entry(tag).or_insert(Vec::new()).push(text);
            }
        }
        if let Some(parens) = get_match(&caps, "parens") {
            let caps_tag = RGX_TAG.captures(&parens);
            if let Some(version_next) = get_match(&caps_tag, "version") {
                add_chunks(&caps, &mut chunks, &version, &version_next, &url);
                version = Some(version_next);
            }
        }
        if i == last {
            add_chunks(&caps, &mut chunks, &version, "main", &url);
        }
    }

    let mut file = File::create("CHANGELOG.md").unwrap();
    writeln!(file, "{}", get_changelog(&chunks)).unwrap();
}
