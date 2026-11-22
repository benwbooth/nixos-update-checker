//! New major NixOS release detection.
//!
//! The flake's `nixpkgs` input names the release it follows
//! (`github:NixOS/nixpkgs/nixos-26.05`). `nix flake update` never moves it to
//! the next release, so that takes an edit to flake.nix; this tells when one
//! is out. A release counts once its channel on channels.nixos.org points at
//! a release build: during the beta weeks the channel already exists but its
//! builds are named `nixos-26.11beta…`.

use std::path::Path;
use std::process::Command;

/// "26.05" from a ref like "nixos-26.05" or "release-26.05"; None for
/// unstable or anything else (nothing to warn about).
fn release_of_ref(r: &str) -> Option<(u32, u32)> {
    let v = r.strip_prefix("nixos-").or_else(|| r.strip_prefix("release-"))?;
    let (y, m) = v.split_once('.')?;
    let (y, m) = (y.parse().ok()?, m.parse().ok()?);
    (m == 5 || m == 11).then_some((y, m))
}

/// The release the flake's root `nixpkgs` input follows, from flake.lock.
pub fn followed_release(flake_path: &Path) -> Option<(u32, u32)> {
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(flake_path.join("flake.lock")).ok()?).ok()?;
    let nodes = lock.get("nodes")?;
    let root = lock.get("root")?.as_str()?;
    let input = nodes.get(root)?.get("inputs")?.get("nixpkgs")?.as_str()?;
    let original = nodes.get(input)?.get("original")?;
    release_of_ref(original.get("ref")?.as_str()?)
}

fn next((y, m): (u32, u32)) -> (u32, u32) {
    if m == 5 { (y, 11) } else { (y + 1, 5) }
}

pub fn fmt((y, m): (u32, u32)) -> String {
    format!("{y:02}.{m:02}")
}

/// Whether a release is out: its channel redirects to a non-beta build.
fn released(release: (u32, u32)) -> bool {
    let url = format!("https://channels.nixos.org/nixos-{}", fmt(release));
    let Ok(out) = Command::new("curl")
        .args(["-sI", "--max-time", "15", &url])
        .output()
    else {
        return false;
    };
    let headers = String::from_utf8_lossy(&out.stdout).to_lowercase();
    headers
        .lines()
        .find_map(|l| l.strip_prefix("location:"))
        .is_some_and(|loc| loc.contains(&format!("/nixos-{}", fmt(release))) && !loc.contains("beta"))
}

/// The newest NixOS release newer than the one the flake follows, if any.
/// Blocking (network): run off the UI thread.
pub fn newer_release(flake_path: &Path) -> Option<String> {
    let current = followed_release(flake_path)?;
    let mut newest = None;
    let mut candidate = next(current);
    // At most two releases ahead (a year behind).
    for _ in 0..2 {
        if !released(candidate) {
            break;
        }
        newest = Some(candidate);
        candidate = next(candidate);
    }
    newest.map(fmt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_refs() {
        assert_eq!(release_of_ref("nixos-26.05"), Some((26, 5)));
        assert_eq!(release_of_ref("release-25.11"), Some((25, 11)));
        assert_eq!(release_of_ref("nixos-unstable"), None);
        assert_eq!(release_of_ref("master"), None);
        assert_eq!(fmt(next((26, 5))), "26.11");
        assert_eq!(fmt(next((26, 11))), "27.05");
    }
}
