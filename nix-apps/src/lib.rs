//! The apps manifest of a NixOS flake (`apps.json`): what is installed from
//! nixpkgs, from Flatpak and as AppImages, in one file the flake reads
//! (`nix/apps.nix`) and programs edit (the update checker, a store app).
//!
//! AppImages are pinned there by URL and hash, so a rebuild fetches exactly
//! what the manifest says. Each one also names where its releases come from
//! (GitHub/GitLab releases, an Electron update feed, zsync update info, or a
//! download page), and [`check`] / [`apply`] move it to the newest release.
//! Versioned download URLs are the norm, which is why this isn't done with
//! flake inputs: those need a URL that stays the same across releases.

pub mod appimage;
pub mod cli;
mod http;
pub mod manifest;
pub mod source;

pub use manifest::{AppImage, Flatpak, Manifest, Source};
pub use source::Release;

use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A newer release of a manifest AppImage.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Update {
    /// The manifest key ("redact")
    pub id: String,
    /// Display name ("Redact")
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    pub url: String,
}

/// Where the manifest of a flake lives.
pub fn manifest_path(flake: &Path) -> std::path::PathBuf {
    flake.join("apps.json")
}

/// Pending AppImage updates, and the AppImages whose source couldn't be read
/// (id, why). Network only: nothing is downloaded but release metadata.
/// Blocking; one thread per AppImage.
pub fn check(manifest: &Manifest) -> (Vec<Update>, Vec<(String, String)>) {
    let results: Vec<(String, Result<Option<Release>>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = manifest
            .appimages
            .iter()
            .map(|(id, app)| (id.clone(), scope.spawn(move || source::latest(&app.source))))
            .collect();
        handles
            .into_iter()
            .map(|(id, h)| (id, h.join().unwrap_or_else(|_| Err(Error("checker thread panicked".into())))))
            .collect()
    });
    let mut updates = Vec::new();
    let mut errors = Vec::new();
    for (id, result) in results {
        let app = &manifest.appimages[&id];
        match result {
            Ok(Some(release)) if release.is_newer_than(app) => updates.push(Update {
                id: id.clone(),
                name: app.name.clone(),
                old_version: app.version.clone(),
                new_version: release.version,
                url: release.url,
            }),
            Ok(_) => {}
            Err(e) => errors.push((id, e.0)),
        }
    }
    updates.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (updates, errors)
}

/// Moves the named AppImages (all of them when `ids` is empty) to their
/// newest release in the manifest at `path`, downloading each new file into
/// the Nix store to hash it (so the rebuild that follows doesn't download it
/// again). Saves once at the end, and only if something changed. `log` gets
/// a line per AppImage. Returns what was updated.
pub fn apply(path: &Path, ids: &[String], mut log: impl FnMut(&str)) -> Result<Vec<Update>> {
    let mut manifest = Manifest::load(path)?;
    for id in ids {
        if !manifest.appimages.contains_key(id) {
            return Err(Error(format!("{id}: not in {}", path.display())));
        }
    }
    let wanted: Vec<String> = if ids.is_empty() {
        manifest.appimages.keys().cloned().collect()
    } else {
        ids.to_vec()
    };
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for id in wanted {
        let app = manifest.appimages.get_mut(&id).expect("checked above");
        let release = match source::latest(&app.source) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                log(&format!("{}: can't check for updates: {e}", app.name));
                failed.push(id.clone());
                continue;
            }
        };
        if !release.is_newer_than(app) {
            log(&format!("{} {} is up to date", app.name, app.version));
            continue;
        }
        log(&format!("{}: downloading {}…", app.name, release.version));
        let hash = match release.hash.clone() {
            Some(h) => {
                // Still fetch now: checks the hash, and the rebuild finds it in the store.
                source::prefetch(&release.url, &store_name(&id, &release.version), Some(&h)).map(|_| h)
            }
            None => source::prefetch(&release.url, &store_name(&id, &release.version), None),
        };
        let hash = match hash {
            Ok(h) => h,
            Err(e) => {
                log(&format!("{}: download failed: {e}", app.name));
                failed.push(id.clone());
                continue;
            }
        };
        log(&format!("{}: {} → {}", app.name, app.version, release.version));
        done.push(Update {
            id: id.clone(),
            name: app.name.clone(),
            old_version: app.version.clone(),
            new_version: release.version.clone(),
            url: release.url.clone(),
        });
        app.version = release.version;
        app.url = release.url;
        app.hash = hash;
    }
    if !done.is_empty() {
        manifest.save(path)?;
    }
    if !failed.is_empty() {
        return Err(Error(format!("couldn't update: {}", failed.join(", "))));
    }
    Ok(done)
}

/// The store name a manifest AppImage is fetched under; nix/apps.nix uses the
/// same, so a file fetched here is the one the rebuild wants.
pub fn store_name(id: &str, version: &str) -> String {
    sanitize_store_name(&format!("{id}-{version}.AppImage"))
}

/// What nixpkgs' `lib.strings.sanitizeDerivationName` does: characters a store
/// name can't have become `-`, leading dots go, at most 207 characters.
pub fn sanitize_store_name(name: &str) -> String {
    let trimmed = name.trim_start_matches('.');
    let mut out = String::new();
    let mut last_dash = false;
    for c in trimmed.chars() {
        let ok = c.is_ascii_alphanumeric() || "+-._?=".contains(c);
        if ok {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out: String = out.chars().take(207).collect();
    if out.is_empty() { "unknown".to_string() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_names() {
        assert_eq!(store_name("redact", "0.21.34"), "redact-0.21.34.AppImage");
        assert_eq!(store_name("steam-rom-manager", "2.5 beta"), "steam-rom-manager-2.5-beta.AppImage");
        assert_eq!(sanitize_store_name("..a//b"), "a-b");
    }
}
