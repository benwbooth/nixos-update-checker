//! `apps.json`: the format, shared with nix/apps.nix.
//!
//! ```json
//! {
//!   "version": 1,
//!   "nixpkgs": ["hello", "kdePackages.kate"],
//!   "flatpaks": ["org.example.App", { "appId": "com.example.Other", "origin": "flathub-beta" }],
//!   "appimages": {
//!     "redact": {
//!       "name": "Redact",
//!       "version": "0.21.34",
//!       "url": "https://update-desktop.redact.dev/build/Redact-0.21.34.AppImage",
//!       "hash": "sha512-…",
//!       "source": { "type": "electron", "feed": "https://update-desktop.redact.dev/build/latest-linux.yml" }
//!     }
//!   }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    #[serde(default = "format_version")]
    pub version: u32,
    /// Attribute paths in nixpkgs ("hello", "kdePackages.kate")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nixpkgs: Vec<String>,
    /// Flatpak apps, as nix-flatpak's `services.flatpak.packages` takes them
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flatpaks: Vec<Flatpak>,
    /// AppImages by id (a package name: lowercase, dashes)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub appimages: BTreeMap<String, AppImage>,
}

fn format_version() -> u32 {
    1
}

impl Default for Manifest {
    fn default() -> Self {
        Self { version: format_version(), nixpkgs: Vec::new(), flatpaks: Vec::new(), appimages: BTreeMap::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Flatpak {
    Id(String),
    Ref {
        #[serde(rename = "appId")]
        app_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
    },
}

impl Flatpak {
    pub fn app_id(&self) -> &str {
        match self {
            Flatpak::Id(id) => id,
            Flatpak::Ref { app_id, .. } => app_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppImage {
    pub name: String,
    pub version: String,
    /// The pinned download (usually versioned)
    pub url: String,
    /// SRI hash of the file ("sha256-…" or "sha512-…")
    pub hash: String,
    /// Where newer releases are found
    pub source: Source,
    /// Extra nixpkgs attributes the app needs in its FHS environment
    #[serde(default, rename = "extraPkgs", skip_serializing_if = "Vec::is_empty")]
    pub extra_pkgs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// Where an AppImage's releases come from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    /// GitHub releases of `owner/repo`; the asset is the one matching the
    /// glob (x86_64 preferred when several do)
    Github {
        repo: String,
        #[serde(default = "default_asset")]
        asset: String,
        /// Take pre-releases too
        #[serde(default, skip_serializing_if = "is_false")]
        prerelease: bool,
    },
    /// GitLab releases of a project ("es-de/emulationstation-de")
    Gitlab {
        #[serde(default = "gitlab_host", skip_serializing_if = "is_gitlab_com")]
        host: String,
        project: String,
        #[serde(default = "default_asset")]
        asset: String,
    },
    /// An electron-builder feed (`…/latest-linux.yml`): version, file, sha512
    Electron { feed: String },
    /// zsync update info (`…/app.AppImage.zsync`), as AppImageUpdate uses
    Zsync { url: String },
    /// A download page and a regex matching the AppImage link on it. A group
    /// named `version` (else the first group) is the version; a group named
    /// `url` (else the whole match) is the link.
    Page { url: String, pattern: String },
    /// Pinned: never checked
    Manual,
}

fn default_asset() -> String {
    "*.AppImage".to_string()
}

fn gitlab_host() -> String {
    "gitlab.com".to_string()
}

fn is_gitlab_com(h: &str) -> bool {
    h == "gitlab.com"
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Source {
    /// "GitHub releases of owner/repo", for messages
    pub fn describe(&self) -> String {
        match self {
            Source::Github { repo, .. } => format!("GitHub releases of {repo}"),
            Source::Gitlab { host, project, .. } => format!("{host} releases of {project}"),
            Source::Electron { feed } => format!("update feed {feed}"),
            Source::Zsync { url } => format!("zsync {url}"),
            Source::Page { url, .. } => format!("download page {url}"),
            Source::Manual => "pinned".to_string(),
        }
    }
}

impl Manifest {
    /// Loads the manifest; a missing file is an empty manifest.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| Error(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error(format!("{}: {e}", path.display()))),
        }
    }

    /// Writes the manifest (two-space JSON, keys sorted) through a temporary
    /// file and a rename, keeping the old file's permissions.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, text)?;
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// A package-name id from an app name: "Steam ROM Manager" → "steam-rom-manager".
pub fn id_from_name(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let json = r#"{
  "version": 1,
  "nixpkgs": ["hello"],
  "flatpaks": ["org.example.App", {"appId": "com.example.B", "origin": "flathub-beta"}],
  "appimages": {
    "redact": {
      "name": "Redact", "version": "0.21.34",
      "url": "https://x/Redact-0.21.34.AppImage", "hash": "sha512-abc",
      "source": {"type": "electron", "feed": "https://x/latest-linux.yml"}
    },
    "srm": {
      "name": "Steam ROM Manager", "version": "2.5.29", "url": "u", "hash": "h",
      "source": {"type": "github", "repo": "SteamGridDB/steam-rom-manager"}
    }
  }
}"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.flatpaks[1].app_id(), "com.example.B");
        assert_eq!(
            m.appimages["srm"].source,
            Source::Github { repo: "SteamGridDB/steam-rom-manager".into(), asset: "*.AppImage".into(), prerelease: false }
        );
        let again: Manifest = serde_json::from_str(&serde_json::to_string_pretty(&m).unwrap()).unwrap();
        assert_eq!(m, again);
    }

    #[test]
    fn missing_file_is_empty() {
        assert_eq!(Manifest::load(Path::new("/nonexistent/apps.json")).unwrap(), Manifest::default());
    }

    #[test]
    fn ids() {
        assert_eq!(id_from_name("Steam ROM Manager"), "steam-rom-manager");
        assert_eq!(id_from_name("EmulationStation-DE"), "emulationstation-de");
    }
}
