use std::{collections::HashMap, fs::File, io::Write, process::Command};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};

// regex to match git log output
static RGX_GIT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?P<date>\d{4}-\d{2}-\d{2})  (\((?P<refs>.*)\) )?((?P<scope>\w+): )?(?P<commit>.*)",
    )
    .unwrap()
});

// regex to match git refs
static RGX_REF: Lazy<Regex> = Lazy::new(|| Regex::new(r"tag: (?P<version>[v0-9.]+)").unwrap());

// valid scopes and corresponding changelog section title
static VALID_SCOPES: Lazy<Vec<(&str, &str)>> =
    Lazy::new(|| vec![("feat", "Added"), ("refactor", "Changed"), ("fix", "Fixed")]);

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
    rgx.replace(&txt, "").trim_end().to_string()
}

// extract regex matches as strings
fn get_match(caps: &Option<Captures>, kind: &str) -> Option<String> {
    caps.as_ref()
        .and_then(|c| c.name(kind))
        .map(|m| String::from(m.as_str()))
}

// create changelog header for each version
fn get_header(version0: Option<String>, version: &str, url: &str, date: &str) -> Vec<String> {
    let header;
    let version_text = match version {
        "main" => "Unreleased",
        _ => version,
    };
    if let Some(ref version_prev) = version0 {
        header = format!(
            "## [{}]({}/compare/{}...{}) - {}",
            version_text, url, version_prev, version, date
        );
    } else {
        header = format!(
            "## [{}]({}/releases/tag/{}) - {}",
            version_text, url, version, date
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
fn get_chunk(chunk0: &HashMap<String, Vec<String>>, scope: &str, header: &str) -> Vec<String> {
    let mut chunk = Vec::new();
    if let Some(items) = chunk0.get(scope) {
        if !items.is_empty() {
            chunk.append(&mut vec![format!("### {}", header), "".to_string()]);
            for added in items.clone().iter().rev() {
                chunk.push(get_list_bullet(added));
            }
            chunk.push("".to_string());
        }
    }
    chunk
}

// check if changelog chunk already exists
fn has_chunk(chunks: &HashMap<String, Vec<String>>, scope: &str) -> bool {
    chunks.get(scope).map(|v| !v.is_empty()).unwrap_or(false)
}

// check if any changelog chunks exist
fn any_chunks(chunks: &HashMap<String, Vec<String>>) -> bool {
    for (scope, _) in VALID_SCOPES.iter().copied() {
        if has_chunk(chunks, scope) {
            return true;
        }
    }
    false
}

// create changelog from version chunks
fn get_changelog(
    chunks: IndexMap<String, (String, HashMap<String, Vec<String>>)>,
    url: String,
) -> String {
    let mut changelog = vec!["# Changelog".to_string(), "".to_string()];
    for (i, (version, (date, chunk0))) in chunks.iter().enumerate().rev() {
        let version0 = if i > 0 {
            chunks.get_index(i - 1).map(|(k, _)| k.to_string())
        } else {
            None
        };
        if any_chunks(chunk0) {
            changelog.append(&mut get_header(version0, version, &url, date));
            for (scope, header) in VALID_SCOPES.iter().copied() {
                changelog.append(&mut get_chunk(chunk0, scope, header));
            }
        }
    }
    changelog.join("\n").trim_end().to_string()
}

fn main() {
    let url = git_remote_url();
    let log = git_log();

    let mut chunks: IndexMap<String, (String, HashMap<String, Vec<String>>)> = IndexMap::new();
    let mut chunk0: HashMap<String, Vec<String>> = HashMap::new();
    let mut date = String::new();

    for line in log.iter().rev() {
        let caps_line = RGX_GIT.captures(line);
        // use most recent date for changelog sections
        if let Some(date_next) = get_match(&caps_line, "date") {
            date = date_next;
        }
        // add commit to section chunk map
        if let (Some(scope), Some(commit)) = (
            get_match(&caps_line, "scope"),
            get_match(&caps_line, "commit"),
        ) {
            chunk0.entry(scope).or_default().push(commit);
        }
        // add all scope-specific commits when there is a valid version
        if let Some(refs) = get_match(&caps_line, "refs") {
            let caps_tag = RGX_REF.captures(&refs);
            if let Some(version) = get_match(&caps_tag, "version") {
                // if version 1.0.0 has no entry, add a default one
                if (version == "v1.0.0" || version == "1.0.0") && !any_chunks(&chunk0) {
                    chunk0
                        .entry("feat".to_string())
                        .or_default()
                        .push("initial release".to_string());
                }
                chunks.insert(version.clone(), (date.clone(), chunk0.clone()));
                chunk0.clear();
            }
        }
    }
    chunks.insert(String::from("main"), (date, chunk0));

    let mut file = File::create("CHANGELOG.md").unwrap();
    writeln!(file, "{}", get_changelog(chunks, url)).unwrap();
}
