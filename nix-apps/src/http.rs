//! HTTP through curl (on PATH in both the dev shell and the package), so the
//! crate needs no TLS stack of its own.

use std::process::Command;

use crate::{Error, Result};

fn curl(url: &str, extra: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--max-time", "30", "--retry", "2", "-A", "nix-apps"]);
    // Unauthenticated GitHub API calls are limited to 60 an hour.
    if url.starts_with("https://api.github.com/") {
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.is_empty() {
                cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
            }
        }
        cmd.args(["-H", "Accept: application/vnd.github+json"]);
    }
    cmd.args(extra).arg("--").arg(url);
    let out = cmd.output().map_err(|e| Error(format!("curl: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim().trim_start_matches("curl: ");
        return Err(Error(format!("{url}: {}", if err.is_empty() { "download failed" } else { err })));
    }
    Ok(out.stdout)
}

pub fn get(url: &str) -> Result<String> {
    Ok(String::from_utf8_lossy(&curl(url, &[])?).into_owned())
}

/// The first `bytes` of a resource (a zsync header; servers that ignore
/// Range send it all, which is still small).
pub fn get_head(url: &str, bytes: usize) -> Result<String> {
    let range = format!("0-{}", bytes.saturating_sub(1));
    Ok(String::from_utf8_lossy(&curl(url, &["-r", &range])?).into_owned())
}

pub fn get_json(url: &str) -> Result<serde_json::Value> {
    serde_json::from_str(&get(url)?).map_err(|e| Error(format!("{url}: {e}")))
}

/// `rel` resolved against `base` the way a browser would.
pub fn join(base: &str, rel: &str) -> String {
    if rel.contains("://") {
        return rel.to_string();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("https", base));
    let host_end = rest.find('/').unwrap_or(rest.len());
    let host = &rest[..host_end];
    if let Some(r) = rel.strip_prefix("//") {
        return format!("{scheme}://{r}");
    }
    if rel.starts_with('/') {
        return format!("{scheme}://{host}{rel}");
    }
    let path = &rest[host_end..];
    let path = path.split(['?', '#']).next().unwrap_or("");
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "/",
    };
    format!("{scheme}://{host}{dir}{rel}")
}

/// Percent-encodes a path segment (a GitLab project path).
pub fn encode_segment(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins() {
        assert_eq!(join("https://a.b/x/y.yml", "f.AppImage"), "https://a.b/x/f.AppImage");
        assert_eq!(join("https://a.b/x/y.yml", "/f"), "https://a.b/f");
        assert_eq!(join("https://a.b", "f"), "https://a.b/f");
        assert_eq!(join("https://a.b/x/", "https://c.d/e"), "https://c.d/e");
        assert_eq!(join("https://a.b/x/page?q=1", "f"), "https://a.b/x/f");
    }

    #[test]
    fn encodes() {
        assert_eq!(encode_segment("es-de/emulationstation-de"), "es-de%2Femulationstation-de");
    }
}
