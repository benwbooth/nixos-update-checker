//! The newest release an AppImage's source offers.

use regex::Regex;
use std::process::Command;

use crate::http;
use crate::manifest::{AppImage, Source};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// SRI hash when the source publishes one (Electron feeds, GitHub's asset
    /// digests); otherwise found by downloading
    pub hash: Option<String>,
}

impl Release {
    /// Whether this release is not what the manifest pins. Sources only ever
    /// offer their newest release, so "different" means "newer".
    pub fn is_newer_than(&self, app: &AppImage) -> bool {
        self.url != app.url || self.version != app.version
    }
}

/// The newest release; None for a pinned (manual) AppImage.
pub fn latest(source: &Source) -> Result<Option<Release>> {
    match source {
        Source::Github { repo, asset, prerelease } => github(repo, asset, *prerelease).map(Some),
        Source::Gitlab { host, project, asset } => gitlab(host, project, asset).map(Some),
        Source::Electron { feed } => electron(feed).map(Some),
        Source::Zsync { url } => zsync(url).map(Some),
        Source::Page { url, pattern } => page(url, pattern).map(Some),
        Source::Manual => Ok(None),
    }
}

/// "v1.2.3" → "1.2.3"
fn version_of_tag(tag: &str) -> String {
    let t = tag.trim();
    match t.strip_prefix('v').or_else(|| t.strip_prefix('V')) {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest.to_string(),
        _ => t.to_string(),
    }
}

/// Shell-style glob, case-insensitive.
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut re = String::from("(?i)^");
    for c in pattern.chars() {
        match c {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    Regex::new(&re).map(|r| r.is_match(text)).unwrap_or(false)
}

const OTHER_ARCHES: &[&str] = &["aarch64", "arm64", "armhf", "armv7", "i386", "i686"];

/// Of the asset names, the AppImage for this machine: matching the glob, not
/// for another architecture (unless the glob asks for it), x86_64 first.
pub fn pick_asset<'a>(names: impl IntoIterator<Item = &'a str>, glob: &str) -> Option<&'a str> {
    let glob_lower = glob.to_lowercase();
    let mut candidates: Vec<&str> = names
        .into_iter()
        .filter(|n| glob_match(glob, n))
        .filter(|n| !n.to_lowercase().ends_with(".zsync"))
        .filter(|n| {
            let l = n.to_lowercase();
            !OTHER_ARCHES.iter().any(|a| l.contains(a) && !glob_lower.contains(a))
        })
        .collect();
    candidates.sort_by_key(|n| {
        let l = n.to_lowercase();
        !(l.contains("x86_64") || l.contains("amd64") || l.contains("x64"))
    });
    candidates.first().copied()
}

/// "sha256:<hex>" (GitHub's asset digest) → "sha256-<base64>"
fn sri_from_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..32).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect();
    Some(format!("sha256-{}", base64(&bytes?)))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn github(repo: &str, asset: &str, prerelease: bool) -> Result<Release> {
    let release = if prerelease {
        let all = http::get_json(&format!("https://api.github.com/repos/{repo}/releases?per_page=10"))?;
        all.as_array()
            .and_then(|a| a.iter().find(|r| !r["draft"].as_bool().unwrap_or(false)).cloned())
            .ok_or_else(|| Error(format!("{repo} has no releases")))?
    } else {
        http::get_json(&format!("https://api.github.com/repos/{repo}/releases/latest"))?
    };
    github_release(repo, asset, &release)
}

fn github_release(repo: &str, asset: &str, release: &serde_json::Value) -> Result<Release> {
    let tag = release["tag_name"].as_str().ok_or_else(|| Error(format!("{repo}: release has no tag")))?;
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let name = pick_asset(assets.iter().filter_map(|a| a["name"].as_str()), asset)
        .ok_or_else(|| Error(format!("{repo} {tag}: no asset matches {asset}")))?;
    let a = assets.iter().find(|a| a["name"].as_str() == Some(name)).expect("picked from these");
    Ok(Release {
        version: version_of_tag(tag),
        url: a["browser_download_url"].as_str().unwrap_or_default().to_string(),
        hash: a["digest"].as_str().and_then(sri_from_digest),
    })
}

fn gitlab(host: &str, project: &str, asset: &str) -> Result<Release> {
    let url = format!("https://{host}/api/v4/projects/{}/releases?per_page=5", http::encode_segment(project));
    let releases = http::get_json(&url)?;
    let release = releases
        .as_array()
        .and_then(|a| a.iter().find(|r| !r["upcoming_release"].as_bool().unwrap_or(false)).cloned())
        .ok_or_else(|| Error(format!("{project} has no releases")))?;
    gitlab_release(project, asset, &release)
}

fn gitlab_release(project: &str, asset: &str, release: &serde_json::Value) -> Result<Release> {
    let tag = release["tag_name"].as_str().ok_or_else(|| Error(format!("{project}: release has no tag")))?;
    let links = release["assets"]["links"].as_array().cloned().unwrap_or_default();
    let name = pick_asset(links.iter().filter_map(|l| l["name"].as_str()), asset)
        .ok_or_else(|| Error(format!("{project} {tag}: no asset matches {asset}")))?;
    let l = links.iter().find(|l| l["name"].as_str() == Some(name)).expect("picked from these");
    let url = l["direct_asset_url"].as_str().or_else(|| l["url"].as_str()).unwrap_or_default();
    Ok(Release { version: version_of_tag(tag), url: url.to_string(), hash: None })
}

fn electron(feed: &str) -> Result<Release> {
    parse_electron_feed(feed, &http::get(feed)?)
}

/// electron-builder's latest-linux.yml: top-level `version`, `path` and
/// `sha512` (older feeds), else the first entry of `files`.
pub fn parse_electron_feed(feed: &str, yml: &str) -> Result<Release> {
    let top = |key: &str| {
        yml.lines().find_map(|l| {
            l.strip_prefix(key)
                .and_then(|r| r.strip_prefix(':'))
                .map(|v| v.trim().trim_matches(['\'', '"']).to_string())
        })
    };
    let first_file = |key: &str| {
        yml.lines().find_map(|l| {
            let l = l.trim_start().trim_start_matches("- ");
            l.strip_prefix(key)
                .and_then(|r| r.strip_prefix(':'))
                .map(|v| v.trim().trim_matches(['\'', '"']).to_string())
        })
    };
    let version = top("version").ok_or_else(|| Error(format!("{feed}: no version")))?;
    let path = top("path")
        .or_else(|| first_file("url"))
        .ok_or_else(|| Error(format!("{feed}: no file")))?;
    let sha512 = top("sha512").or_else(|| first_file("sha512"));
    Ok(Release {
        version,
        url: http::join(feed, &path),
        hash: sha512.filter(|s| !s.is_empty()).map(|s| format!("sha512-{s}")),
    })
}

fn zsync(url: &str) -> Result<Release> {
    parse_zsync_header(url, &http::get_head(url, 8192)?)
}

/// The header of a .zsync file: `Filename`, `URL` (relative to the .zsync)
/// and `SHA-1` of the current file. The version comes from the file name, or
/// failing that the hash (which still changes with every release).
pub fn parse_zsync_header(url: &str, header: &str) -> Result<Release> {
    let mut filename = None;
    let mut file_url = None;
    let mut sha1 = None;
    for line in header.lines() {
        if line.is_empty() {
            break;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim().to_string();
        match k.trim() {
            "Filename" => filename = Some(v),
            "URL" => file_url = Some(v),
            "SHA-1" => sha1 = Some(v),
            _ => {}
        }
    }
    let filename = filename.ok_or_else(|| Error(format!("{url}: not a zsync file")))?;
    let version = Regex::new(r"\d+(?:\.\d+)+")
        .unwrap()
        .find(&filename)
        .map(|m| m.as_str().to_string())
        .or_else(|| sha1.as_ref().map(|s| format!("sha1-{}", &s[..s.len().min(12)])))
        .unwrap_or_else(|| filename.clone());
    Ok(Release {
        version,
        url: http::join(url, file_url.as_deref().unwrap_or(&filename)),
        hash: None,
    })
}

fn page(url: &str, pattern: &str) -> Result<Release> {
    parse_page(url, pattern, &http::get(url)?)
}

pub fn parse_page(url: &str, pattern: &str, body: &str) -> Result<Release> {
    let re = Regex::new(pattern).map_err(|e| Error(format!("bad pattern: {e}")))?;
    let caps = re.captures(body).ok_or_else(|| Error(format!("{url}: no link matches {pattern}")))?;
    let link = caps.name("url").or_else(|| caps.get(0)).map(|m| m.as_str()).unwrap_or_default();
    let version = caps
        .name("version")
        .or_else(|| caps.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| Error(format!("{pattern}: no version group")))?;
    Ok(Release { version, url: http::join(url, link), hash: None })
}

/// Downloads `url` into the Nix store under `name`, as fetchurl would, and
/// returns its SRI sha256. With `expected`, Nix checks the file against it.
pub fn prefetch(url: &str, name: &str, expected: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("nix");
    cmd.args(["--extra-experimental-features", "nix-command", "store", "prefetch-file", "--json", "--name", name]);
    if let Some(h) = expected {
        // The check is done with the expected hash's own algorithm.
        let algo = h.split_once('-').map(|(a, _)| a).unwrap_or("sha256");
        cmd.args(["--hash-type", algo, "--expected-hash", h]);
    }
    cmd.arg(url);
    let out = cmd.output().map_err(|e| Error(format!("nix: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("download failed");
        return Err(Error(last.trim().to_string()));
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    json["hash"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error("nix store prefetch-file gave no hash".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_feed() {
        let yml = "version: 0.21.34\nfiles:\n  - url: Redact-0.21.34.AppImage\n    sha512: 14Cu==\n    size: 1\npath: Redact-0.21.34.AppImage\nsha512: 14Cu==\nreleaseDate: '2026-11-24T20:31:19.748Z'\n";
        let r = parse_electron_feed("https://update-desktop.redact.dev/build/latest-linux.yml", yml).unwrap();
        assert_eq!(r.version, "0.21.34");
        assert_eq!(r.url, "https://update-desktop.redact.dev/build/Redact-0.21.34.AppImage");
        assert_eq!(r.hash.as_deref(), Some("sha512-14Cu=="));
        // No top-level path: the first file.
        let r = parse_electron_feed("https://h/f/latest-linux.yml", "version: 1.0.0\nfiles:\n  - url: A-1.0.0.AppImage\n    sha512: xyz\n").unwrap();
        assert_eq!(r.url, "https://h/f/A-1.0.0.AppImage");
        assert_eq!(r.hash.as_deref(), Some("sha512-xyz"));
    }

    #[test]
    fn zsync_header() {
        let h = "zsync: 0.6.2\nFilename: Cursor-3.2.16-x86_64.AppImage\nMTime: x\nBlocksize: 2048\nLength: 1\nURL: Cursor-3.2.16-x86_64.AppImage\nSHA-1: 0123456789abcdef0123\n\n\u{1}\u{2}binary";
        let r = parse_zsync_header("https://d.example/linux/cursor-latest.AppImage.zsync", h).unwrap();
        assert_eq!(r.version, "3.2.16");
        assert_eq!(r.url, "https://d.example/linux/Cursor-3.2.16-x86_64.AppImage");
        let r = parse_zsync_header("https://d/x.zsync", "Filename: app.AppImage\nSHA-1: abcdef0123456789\n\n").unwrap();
        assert_eq!(r.version, "sha1-abcdef012345");
    }

    #[test]
    fn page_links() {
        let body = r#"<a href="/dl/Tool-4.2.0-x86_64.AppImage">get</a>"#;
        let r = parse_page("https://t.example/download", r#"/dl/Tool-(?P<version>[\d.]+)-x86_64\.AppImage"#, body).unwrap();
        assert_eq!(r.version, "4.2.0");
        assert_eq!(r.url, "https://t.example/dl/Tool-4.2.0-x86_64.AppImage");
    }

    #[test]
    fn assets() {
        let names = ["App-1.0-arm64.AppImage", "App-1.0.AppImage.zsync", "App-1.0-x86_64.AppImage", "App-1.0.dmg"];
        assert_eq!(pick_asset(names, "*.AppImage"), Some("App-1.0-x86_64.AppImage"));
        assert_eq!(pick_asset(names, "*arm64*.appimage"), Some("App-1.0-arm64.AppImage"));
        assert_eq!(pick_asset(["Steam-ROM-Manager-2.5.29.AppImage"], "*.AppImage"), Some("Steam-ROM-Manager-2.5.29.AppImage"));
    }

    #[test]
    fn github_digest_to_sri() {
        let d = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert_eq!(sri_from_digest(d).as_deref(), Some("sha256-n4bQgYhMfWWaL+qgxVrQFaO/TxsrC4Is0V1sFbDwCgg="));
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn tags() {
        assert_eq!(version_of_tag("v2.5.29"), "2.5.29");
        assert_eq!(version_of_tag("3.1.0"), "3.1.0");
        assert_eq!(version_of_tag("vivaldi"), "vivaldi");
    }

    #[test]
    fn github_release_json() {
        let j = serde_json::json!({"tag_name": "v2.5.29", "assets": [
            {"name": "Steam-ROM-Manager-2.5.29.AppImage", "browser_download_url": "https://g/srm.AppImage",
             "digest": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"},
            {"name": "Steam-ROM-Manager-Setup-2.5.29.exe", "browser_download_url": "https://g/srm.exe"}]});
        let r = github_release("SteamGridDB/steam-rom-manager", "*.AppImage", &j).unwrap();
        assert_eq!(r.version, "2.5.29");
        assert_eq!(r.url, "https://g/srm.AppImage");
        assert!(r.hash.unwrap().starts_with("sha256-"));
    }
}
