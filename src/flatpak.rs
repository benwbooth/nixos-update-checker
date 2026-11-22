//! Flatpak update detection, for both the system installation (what the
//! system flake's nix-flatpak manages) and the user's own installation.
//!
//! `flatpak remote-ls --updates` lists refs with a newer commit on their
//! remote. Masked refs (`flatpak mask`, e.g. a version frozen on purpose) are
//! left out, since `flatpak update` skips them too.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlatpakUpdate {
    /// Display name ("Jellyfin Desktop"); falls back to the app ID.
    pub name: String,
    pub app_id: String,
    pub branch: String,
    pub origin: String,
    /// "system" or "user"
    pub installation: String,
    /// "app" or "runtime"
    pub kind: String,
    #[serde(default)]
    pub old_version: String,
    #[serde(default)]
    pub new_version: String,
    /// Full ref ("app/org.example.App/x86_64/stable"), what `flatpak update` takes
    #[serde(default)]
    pub full_ref: String,
    /// From `flatpak remote-info`: "142.0 MB"
    #[serde(default)]
    pub download_size: String,
    /// One-line description of the app ("Jellyfin desktop client")
    #[serde(default)]
    pub summary: String,
    /// The new commit's subject: the closest thing to release notes
    #[serde(default)]
    pub subject: String,
    /// The new commit's date ("2026-11-21 07:35:15 +0000")
    #[serde(default)]
    pub date: String,
}

impl FlatpakUpdate {
    pub fn display(&self) -> String {
        match (self.old_version.as_str(), self.new_version.as_str()) {
            (old, new) if !old.is_empty() && !new.is_empty() && old != new => {
                format!("{} ({} → {})", self.name, old, new)
            }
            (_, new) if !new.is_empty() => format!("{} ({})", self.name, new),
            _ => format!("{} ({})", self.name, self.branch),
        }
    }
}

fn flatpak(args: &[&str]) -> Option<String> {
    let output = Command::new("flatpak").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Shell-style glob (`*`, `?`) match, as flatpak uses for mask patterns.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether a ref ("app/com.example.App/x86_64/stable") is masked. Patterns
/// may name the app ID alone, `id//branch`, or a full or partial ref.
fn is_masked(masks: &[String], kind: &str, app_id: &str, arch: &str, branch: &str) -> bool {
    let full = format!("{kind}/{app_id}/{arch}/{branch}");
    let partial = format!("{app_id}/{arch}/{branch}");
    let id_branch = format!("{app_id}//{branch}");
    masks.iter().any(|m| {
        [app_id, id_branch.as_str(), partial.as_str(), full.as_str()]
            .iter()
            .any(|candidate| glob_match(m, candidate))
    })
}

fn masks(installation: &str) -> Vec<String> {
    flatpak(&["mask", &format!("--{installation}")])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Installed versions, keyed by "id/arch/branch".
fn installed_versions(installation: &str) -> HashMap<String, String> {
    flatpak(&["list", &format!("--{installation}"), "--columns=ref,version"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let r = cols.next()?.trim();
            let version = cols.next().unwrap_or("").trim();
            Some((r.to_string(), version.to_string()))
        })
        .collect()
}

fn parse_updates(
    output: &str,
    installation: &str,
    masks: &[String],
    installed: &HashMap<String, String>,
) -> Vec<FlatpakUpdate> {
    output
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
            let [name, app_id, version, branch, origin, r] = cols.as_slice() else {
                return None;
            };
            let mut parts = r.split('/');
            let kind = parts.next()?;
            let _id = parts.next()?;
            let arch = parts.next().unwrap_or("");
            if kind != "app" && kind != "runtime" {
                return None;
            }
            if is_masked(masks, kind, app_id, arch, branch) {
                return None;
            }
            let old_version = installed
                .get(&format!("{app_id}/{arch}/{branch}"))
                .cloned()
                .unwrap_or_default();
            Some(FlatpakUpdate {
                name: if name.is_empty() { app_id.to_string() } else { name.to_string() },
                app_id: app_id.to_string(),
                branch: branch.to_string(),
                origin: origin.to_string(),
                installation: installation.to_string(),
                kind: kind.to_string(),
                old_version,
                new_version: version.to_string(),
                full_ref: r.to_string(),
                download_size: String::new(),
                summary: String::new(),
                subject: String::new(),
                date: String::new(),
            })
        })
        .collect()
}

/// Fills in size, summary and latest change from `flatpak remote-info`.
fn add_remote_info(u: &mut FlatpakUpdate) {
    let Some(out) = flatpak(&["remote-info", &format!("--{}", u.installation), &u.origin, &u.full_ref])
    else {
        return;
    };
    let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
    // First line: "Name - summary".
    if let Some(first) = lines.next() {
        if let Some((_, summary)) = first.split_once(" - ") {
            u.summary = summary.trim().to_string();
        }
    }
    for line in out.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().to_string();
        match key.trim() {
            "Download Size" | "Download" => u.download_size = value,
            "Subject" => u.subject = value,
            "Date" => u.date = value,
            _ => {}
        }
    }
}

/// All pending Flatpak updates. Never fails: no flatpak, no network or no
/// installation just means nothing to report. Blocking: run off the UI thread.
pub fn check_updates() -> Vec<FlatpakUpdate> {
    let mut updates = Vec::new();
    for installation in ["system", "user"] {
        let Some(out) = flatpak(&[
            "remote-ls",
            "--updates",
            &format!("--{installation}"),
            "--columns=name,application,version,branch,origin,ref",
        ]) else {
            continue;
        };
        updates.extend(parse_updates(
            &out,
            installation,
            &masks(installation),
            &installed_versions(installation),
        ));
    }
    // Details for each, a few at a time (each is a network round trip).
    for chunk in updates.chunks_mut(6) {
        std::thread::scope(|scope| {
            for u in chunk.iter_mut() {
                scope.spawn(move || add_remote_info(u));
            }
        });
    }
    // Apps first, then runtimes, each by name.
    updates.sort_by(|a, b| (a.kind != "app", a.name.to_lowercase()).cmp(&(b.kind != "app", b.name.to_lowercase())));
    updates
}

pub fn has_system_updates(updates: &[FlatpakUpdate]) -> bool {
    updates.iter().any(|u| u.installation == "system")
}

pub fn has_user_updates(updates: &[FlatpakUpdate]) -> bool {
    updates.iter().any(|u| u.installation == "user")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_match_ids_and_refs() {
        let m = vec!["com.stremio.Stremio".to_string()];
        assert!(is_masked(&m, "app", "com.stremio.Stremio", "x86_64", "stable"));
        assert!(!is_masked(&m, "app", "org.jellyfin.JellyfinDesktop", "x86_64", "stable"));
        let m = vec!["com.stremio.Stremio//stable".to_string()];
        assert!(is_masked(&m, "app", "com.stremio.Stremio", "x86_64", "stable"));
        assert!(!is_masked(&m, "app", "com.stremio.Stremio", "x86_64", "beta"));
        let m = vec!["org.gnome.*".to_string()];
        assert!(is_masked(&m, "runtime", "org.gnome.Platform", "x86_64", "50"));
    }

    #[test]
    fn parses_remote_ls() {
        let out = "Stremio\tcom.stremio.Stremio\t1.3.0\tstable\tflathub\tapp/com.stremio.Stremio/x86_64/stable\n\
                   Jellyfin Desktop\torg.jellyfin.JellyfinDesktop\t2.0.0\tstable\tflathub\tapp/org.jellyfin.JellyfinDesktop/x86_64/stable\n\
                   GNOME Platform\torg.gnome.Platform\t\t50\tflathub\truntime/org.gnome.Platform/x86_64/50\n";
        let masks = vec!["com.stremio.Stremio".to_string()];
        let mut installed = HashMap::new();
        installed.insert("org.jellyfin.JellyfinDesktop/x86_64/stable".to_string(), "1.9.1".to_string());
        let u = parse_updates(out, "system", &masks, &installed);
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].name, "Jellyfin Desktop");
        assert_eq!(u[0].old_version, "1.9.1");
        assert_eq!(u[0].display(), "Jellyfin Desktop (1.9.1 → 2.0.0)");
        assert_eq!(u[1].kind, "runtime");
        assert_eq!(u[1].display(), "GNOME Platform (50)");
    }
}
