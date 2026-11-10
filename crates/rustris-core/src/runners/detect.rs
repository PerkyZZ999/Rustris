//! D-012: list the UMU launchers, Proton builds and prepared runtimes already
//! installed. Detection only reads bounded metadata from well-known locations; it
//! never runs, downloads, creates or updates anything.
use super::umu::{
    GE_PROTON_VERSION, PROTON_VERSION, RUNTIME_VERSION, RuntimeKind, STEAMRT4_VERSION,
};
use crate::bytes::OsBytes;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 256;
const MAX_SCANNED: usize = 20_000;
const MAX_TEXT: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Detected {
    pub umu: Vec<Tool>,
    pub proton: Vec<Proton>,
    pub runtimes: Vec<Runtime>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Tool {
    pub path: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Proton {
    pub path: String,
    /// The name from the build's `version` file, e.g. `GE-Proton11-5`.
    pub name: String,
    /// The name, plus the folder when two builds report the same name.
    pub label: String,
    /// `steamrt3` or `steamrt4`: the runtime directory this build needs, if known.
    pub needs: Option<&'static str>,
    pub needs_label: Option<&'static str>,
    /// Part of a validated set (D-006, D-012); others are allowed but untested.
    pub validated: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Runtime {
    pub path: String,
    pub directory: &'static str,
    pub label: &'static str,
    pub version: String,
    pub validated: bool,
}

/// `data_home` is `$XDG_DATA_HOME`; `home` is `$HOME`; `search_path` is `$PATH`.
pub fn detect(data_home: &Path, home: Option<&Path>, search_path: Option<&OsString>) -> Detected {
    let mut detected = Detected::default();
    let mut seen = BTreeSet::new();

    let mut tools: Vec<PathBuf> = Vec::new();
    if let Some(search) = search_path {
        tools.extend(std::env::split_paths(search).map(|dir| dir.join("umu-run")));
    }
    if let Some(home) = home {
        tools.push(home.join(".local/bin/umu-run"));
    }
    tools.extend(["/usr/bin/umu-run", "/usr/local/bin/umu-run"].map(PathBuf::from));
    for candidate in tools.into_iter().take(MAX_ENTRIES) {
        if is_executable_file(&candidate)
            && remember(&mut seen, &candidate)
            && let Some(path) = text(&candidate)
        {
            detected.umu.push(Tool { path });
        }
    }

    let mut roots = vec![
        data_home.join("Steam/compatibilitytools.d"),
        data_home.join("Steam/steamapps/common"),
    ];
    if let Some(home) = home {
        roots.push(home.join(".steam/root/compatibilitytools.d"));
        roots.push(home.join(".steam/steam/compatibilitytools.d"));
        roots.push(home.join(".steam/steam/steamapps/common"));
    }
    roots.push(PathBuf::from("/usr/share/steam/compatibilitytools.d"));
    for root in roots {
        for directory in children(&root) {
            if let Some(proton) = proton(&directory, &mut seen) {
                detected.proton.push(proton);
            }
        }
    }
    detected.proton.sort_by(|a, b| {
        b.validated
            .cmp(&a.validated)
            .then(natural(&a.name, &b.name))
    });

    for name in ["steamrt3", "steamrt4"] {
        let directory = data_home.join("umu").join(name);
        if let Some(runtime) = runtime(&directory, &mut seen) {
            detected.runtimes.push(runtime);
        }
    }
    detected
}

fn proton(directory: &Path, seen: &mut BTreeSet<PathBuf>) -> Option<Proton> {
    if !is_executable_file(&directory.join("proton")) {
        return None;
    }
    let version = read(&directory.join("version"))?;
    let fields: Vec<_> = version.split_whitespace().collect();
    let name = (*fields.last()?).to_owned();
    let manifest = read(&directory.join("toolmanifest.vdf")).unwrap_or_default();
    let needs = manifest
        .lines()
        .filter(|line| line.contains("\"require_tool_appid\""))
        .find_map(|line| line.split_whitespace().nth(1))
        .and_then(|id| match id.trim_matches('"') {
            "1628350" => Some(RuntimeKind::Sniper),
            "4183110" => Some(RuntimeKind::SteamRt4),
            _ => None,
        });
    if !remember(seen, directory) {
        return None;
    }
    let validated = matches!(fields.as_slice(), [_, PROTON_VERSION])
        || matches!(fields.as_slice(), ["1786437966", GE_PROTON_VERSION]);
    Some(Proton {
        path: text(directory)?,
        label: name.clone(),
        name,
        needs: needs.map(|kind| match kind {
            RuntimeKind::Sniper => "steamrt3",
            RuntimeKind::SteamRt4 => "steamrt4",
        }),
        needs_label: needs.map(RuntimeKind::label),
        validated,
    })
}

fn runtime(directory: &Path, seen: &mut BTreeSet<PathBuf>) -> Option<Runtime> {
    if !is_executable_file(&directory.join("_v2-entry-point")) {
        return None;
    }
    let (name, kind) = match directory.file_name()?.as_bytes() {
        b"steamrt3" => ("sniper", RuntimeKind::Sniper),
        b"steamrt4" => ("steamrt4", RuntimeKind::SteamRt4),
        _ => return None,
    };
    let versions = read(&directory.join("VERSIONS.txt"))?;
    let version = versions.lines().find_map(|line| {
        let mut cells = line.split('\t');
        (cells.next() == Some(name)).then(|| cells.next().unwrap_or_default().trim().to_owned())
    })?;
    if version.is_empty() || !remember(seen, directory) {
        return None;
    }
    let validated = match kind {
        RuntimeKind::Sniper => version == RUNTIME_VERSION,
        RuntimeKind::SteamRt4 => version == STEAMRT4_VERSION,
    };
    Some(Runtime {
        path: text(directory)?,
        directory: if kind == RuntimeKind::Sniper {
            "steamrt3"
        } else {
            "steamrt4"
        },
        label: kind.label(),
        version,
        validated,
    })
}

fn children(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    // A Steam library can hold thousands of game folders; scan generously and keep
    // only folders that look like tools, so Proton is not missed behind games.
    entries
        .take(MAX_SCANNED)
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_dir() && (path.join("proton").exists() || path.join("_v2-entry-point").exists())
        })
        .take(MAX_ENTRIES)
        .collect()
}
fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file())
        && rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
}
/// Deduplicate by canonical path (symlinked Steam roots point at the same tree).
fn remember(seen: &mut BTreeSet<PathBuf>, path: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|canonical| seen.insert(canonical))
}
fn read(path: &Path) -> Option<String> {
    let mut contents = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_TEXT)
        .read_to_string(&mut contents)
        .ok()?;
    Some(contents)
}
/// Paths the UI can show and save as text; others need the manual byte fields.
fn text(path: &Path) -> Option<String> {
    OsBytes::new(path.as_os_str().as_bytes()).ok()?;
    path.to_str().map(str::to_owned)
}
/// Orders `GE-Proton11-12` before `GE-Proton11-5` (newest first).
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(s: &str) -> Vec<(String, u64)> {
        let mut parts = Vec::new();
        let mut word = String::new();
        let mut number = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                number.push(c);
            } else {
                if !number.is_empty() {
                    parts.push((
                        std::mem::take(&mut word),
                        number.parse().unwrap_or(u64::MAX),
                    ));
                    number.clear();
                }
                word.push(c);
            }
        }
        parts.push((word, number.parse().unwrap_or(0)));
        parts
    }
    // Families alphabetically; newest first within a family. The key comparison is a
    // total order (parts in turn, then the part count, then the raw text) so sorting
    // can never see an inconsistent comparison.
    let (key_a, key_b) = (key(a), key(b));
    for ((word_a, number_a), (word_b, number_b)) in key_a.iter().zip(key_b.iter()) {
        let order = word_a
            .to_lowercase()
            .cmp(&word_b.to_lowercase())
            .then(number_b.cmp(number_a));
        if order.is_ne() {
            return order;
        }
    }
    key_a.len().cmp(&key_b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "Private fixture assertions")]
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn executable(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn proton_build(root: &Path, name: &str, version: &str, appid: &str) {
        let dir = root.join(name);
        executable(&dir.join("proton"));
        fs::write(dir.join("version"), version).unwrap();
        fs::write(
            dir.join("toolmanifest.vdf"),
            format!("\"manifest\"\n{{\n  \"require_tool_appid\" \"{appid}\"\n}}\n"),
        )
        .unwrap();
    }

    #[test]
    fn lists_installed_tools_marks_validated_sets_and_ignores_incomplete_ones() {
        let root = std::env::temp_dir().join(format!("rustris-detect-{}", uuid::Uuid::now_v7()));
        let data = root.join("data");
        let tools = data.join("Steam/compatibilitytools.d");
        proton_build(
            &tools,
            "GE-Proton11-5-x86_64",
            "1786437966 GE-Proton11-5\n",
            "4183110",
        );
        proton_build(
            &tools,
            "GE-Proton11-12-x86_64",
            "1790000000 GE-Proton11-12\n",
            "4183110",
        );
        proton_build(
            &tools,
            "proton-cachyos",
            "1 proton-cachyos-10.0\n",
            "1628350",
        );
        fs::create_dir_all(tools.join("incomplete")).unwrap();
        let rt = data.join("umu/steamrt4");
        executable(&rt.join("_v2-entry-point"));
        fs::write(
            rt.join("VERSIONS.txt"),
            "#Name\tVersion\nsteamrt4\t4.0.20261021.265997\n",
        )
        .unwrap();
        let bin = root.join("bin");
        executable(&bin.join("umu-run"));
        let search = std::env::join_paths([&bin, &bin]).unwrap();

        let found = detect(&data, None, Some(&search));
        let fixture = bin.join("umu-run").to_str().unwrap().to_owned();
        assert_eq!(
            found.umu.iter().filter(|tool| tool.path == fixture).count(),
            1,
            "duplicate PATH entries collapse"
        );
        assert_eq!(found.umu[0].path, fixture, "PATH order is kept");
        // The host's own /usr/share builds may also appear; check only the fixture's.
        let ours: Vec<_> = found
            .proton
            .iter()
            .filter(|p| p.path.starts_with(tools.to_str().unwrap()))
            .collect();
        let names: Vec<_> = ours.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["GE-Proton11-5", "GE-Proton11-12", "proton-cachyos-10.0"],
            "validated first, then newest first by family"
        );
        assert!(ours[0].validated && !ours[1].validated);
        assert_eq!(ours[0].needs, Some("steamrt4"));
        assert_eq!(ours[2].needs, Some("steamrt3"));
        assert_eq!(found.runtimes.len(), 1);
        assert!(found.runtimes[0].validated);
        assert_eq!(found.runtimes[0].version, "4.0.20261021.265997");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ordering_is_total_for_awkward_names() {
        let mut names = [
            "a00000",
            "a0b",
            "a00000a",
            "GE-Proton9-1",
            "ge-proton10-2",
            "a0b",
        ];
        // Would panic or loop with an inconsistent comparison.
        names.sort_by(|a, b| natural(a, b));
        for pair in names.windows(2) {
            assert_ne!(natural(pair[0], pair[1]), std::cmp::Ordering::Greater);
        }
    }
}
