use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Try to locate `BackpackBattles.pck` across all Steam libraries.
///
/// This is best-effort and returns the first hit found.
pub fn detect_backpack_battles_pck() -> Option<PathBuf> {
    // Common Steam relative location for the game.
    const GAME_DIR: &str = "Backpack Battles";
    const PCK_NAME: &str = "BackpackBattles.pck";

    for steam_root in steam_root_candidates() {
        for lib_root in steam_library_roots(&steam_root) {
            let candidate = lib_root
                .join("steamapps")
                .join("common")
                .join(GAME_DIR)
                .join(PCK_NAME);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn steam_root_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    {
        if let Some(p) = steam_root_from_registry() {
            candidates.push(p);
        }

        // Fallbacks for typical installs.
        candidates.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
        candidates.push(PathBuf::from(r"C:\Program Files\Steam"));
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            candidates.push(home.join("Library/Application Support/Steam"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home_dir() {
            candidates.push(home.join(".local/share/Steam"));
            candidates.push(home.join(".steam/steam"));
        }
    }

    // Deduplicate and keep only existing directories.
    dedup_paths(
        candidates
            .into_iter()
            .filter(|p| p.is_dir())
            .collect::<Vec<_>>(),
    )
}

fn steam_library_roots(steam_root: &Path) -> Vec<PathBuf> {
    // Always include Steam install dir itself as a library root.
    let mut roots = vec![steam_root.to_path_buf()];

    let vdf_path = steam_root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(content) = fs::read_to_string(&vdf_path) {
        roots.extend(parse_libraryfolders_vdf(&content));
    }

    dedup_paths(roots.into_iter().filter(|p| p.is_dir()).collect())
}

fn parse_libraryfolders_vdf(content: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = parse_quoted_kv_pair(line) else {
            continue;
        };

        if key.eq_ignore_ascii_case("path") {
            if !value.is_empty() {
                paths.push(PathBuf::from(value));
            }
            continue;
        }

        // Old Steam format: "1"  "D:\\SteamLibrary"
        if key.chars().all(|c| c.is_ascii_digit()) && looks_like_path(&value) {
            paths.push(PathBuf::from(value));
        }
    }

    paths
}

fn looks_like_path(value: &str) -> bool {
    // Keep this heuristic permissive; false-positives are filtered by `is_dir()` later.
    value.starts_with('/') || value.starts_with("\\\\") || value.contains(":\\") || value.contains(":/")
}

/// Parse a single line containing two quoted strings: `"key" "value"`.
/// Supports backslash-escaped characters inside quotes (e.g. `D:\\SteamLibrary`).
fn parse_quoted_kv_pair(line: &str) -> Option<(String, String)> {
    let mut parts: Vec<String> = Vec::with_capacity(2);
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }

        let mut out = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    // VDF escapes backslash and quotes; just take the next char verbatim.
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                '"' => break,
                other => out.push(other),
            }
        }

        parts.push(out);
        if parts.len() == 2 {
            break;
        }
    }

    if parts.len() == 2 {
        Some((parts.remove(0), parts.remove(0)))
    } else {
        None
    }
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();

    for p in paths {
        let key = normalize_path_key(&p);
        if seen.insert(key) {
            out.push(p);
        }
    }

    out
}

fn normalize_path_key(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn home_dir() -> Option<PathBuf> {
    // Keep dependencies minimal; HOME is sufficient for our use cases.
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(windows)]
fn steam_root_from_registry() -> Option<PathBuf> {
    // Avoid a Windows-only dependency by shelling out to `reg.exe`.
    // This keeps macOS/Linux builds (GUI feature) dependency-free.
    const QUERIES: [(&str, &str); 4] = [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKCU\Software\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
    ];

    for (key, value) in QUERIES {
        if let Some(path) = reg_query_string(key, value) {
            let normalized = path.trim().replace('/', "\\");
            if !normalized.is_empty() {
                return Some(PathBuf::from(normalized));
            }
        }
    }

    None
}

#[cfg(windows)]
fn reg_query_string(key: &str, value: &str) -> Option<String> {
    use std::process::Command;

    let out = Command::new("reg")
        .args(["query", key, "/v", value])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Expected: <ValueName> <Type> <Data...>
        // Example: SteamPath    REG_SZ    C:\Program Files (x86)\Steam
        if !trimmed.starts_with(value) {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        // Data may contain spaces; join everything after the type.
        return Some(parts[2..].join(" "));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_new_format_paths() {
        let vdf = r#"
            "libraryfolders"
            {
                "0"
                {
                    "path"      "C:\\Program Files (x86)\\Steam"
                }
                "1"
                {
                    "path"      "D:\\SteamLibrary"
                }
            }
        "#;

        let paths = parse_libraryfolders_vdf(vdf);
        assert!(paths.iter().any(|p| p.to_string_lossy().contains("SteamLibrary")));
        assert!(paths.iter().any(|p| p.to_string_lossy().contains("Steam")));
    }

    #[test]
    fn parse_old_format_paths() {
        let vdf = r#"
            "LibraryFolders"
            {
                "TimeNextStatsReport"    "123"
                "1"    "D:\\SteamLibrary"
                "2"    "E:\\Games\\Steam"
            }
        "#;

        let paths = parse_libraryfolders_vdf(vdf);
        assert!(paths.iter().any(|p| p.to_string_lossy().contains("SteamLibrary")));
        assert!(paths.iter().any(|p| p.to_string_lossy().contains("Games")));
    }

    #[test]
    fn parse_kv_pair_unescapes_backslashes() {
        let line = r#""path" "D:\\SteamLibrary""#;
        let (k, v) = parse_quoted_kv_pair(line).unwrap();
        assert_eq!(k, "path");
        assert_eq!(v, r"D:\SteamLibrary");
    }
}
