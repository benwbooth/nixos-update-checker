use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum FlakeCheckError {
    #[error("Flake path does not exist: {0}")]
    PathNotFound(String),
    #[error("Failed to run nix command: {0}")]
    CommandFailed(#[from] std::io::Error),
    #[error("Nix command returned error: {0}")]
    NixError(String),
    #[error("Not a git repository: {0}")]
    NotGitRepo(String),
}

/// Represents a package that would be updated
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeUpdate {
    pub package_name: String,
    /// Full name of the OLD (currently installed) package
    #[serde(default)]
    pub old_package_name: String,
    /// First 8 chars of the NEW nix store hash
    #[serde(default)]
    pub hash_short: String,
    /// First 8 chars of the OLD (currently installed) nix store hash
    #[serde(default)]
    pub old_hash_short: String,
}

impl FlakeUpdate {
    /// Extract version from a package name (e.g., "firefox-120.0" -> "120.0")
    fn extract_version(name: &str) -> Option<&str> {
        if let Some(pos) = name.rfind('-') {
            let after = &name[pos + 1..];
            if after.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return Some(after);
            }
        }
        None
    }

    /// Get the new version
    pub fn version(&self) -> Option<&str> {
        Self::extract_version(&self.package_name)
    }

    /// Get the old version
    pub fn old_version(&self) -> Option<&str> {
        if self.old_package_name.is_empty() {
            None
        } else {
            Self::extract_version(&self.old_package_name)
        }
    }

    /// Format the update for display in tooltip
    pub fn display(&self) -> String {
        let new_ver = self.version();
        let old_ver = self.old_version();

        match (old_ver, new_ver) {
            // Both have versions and they differ
            (Some(old), Some(new)) if old != new => {
                format!("{} ({} → {})", self.base_name(), old, new)
            }
            // Both have same version, show with hash
            (Some(old), Some(new)) if old == new && !self.old_hash_short.is_empty() && !self.hash_short.is_empty() => {
                format!("{} ({} {} → {})", self.base_name(), old, self.old_hash_short, self.hash_short)
            }
            // Only new has version
            (None, Some(new)) if !self.old_hash_short.is_empty() => {
                format!("{} ({} → {})", self.base_name(), self.old_hash_short, new)
            }
            // Only old has version
            (Some(old), None) if !self.hash_short.is_empty() => {
                format!("{} ({} → {})", self.base_name(), old, self.hash_short)
            }
            // Fall back to hash display
            _ if !self.old_hash_short.is_empty() && !self.hash_short.is_empty() => {
                format!("{} ({} → {})", self.base_name(), self.old_hash_short, self.hash_short)
            }
            // New package
            _ if !self.hash_short.is_empty() => {
                format!("{} (new: {})", self.package_name, self.hash_short)
            }
            _ => self.package_name.clone(),
        }
    }

    /// Get the base name without version (for deduplication)
    pub fn base_name(&self) -> &str {
        // Try to find where the version starts (usually after last dash followed by digit)
        let name = &self.package_name;

        // Find the last segment that looks like a version
        if let Some(pos) = name.rfind('-') {
            let after = &name[pos + 1..];
            // Check if it starts with a digit (version number)
            if after.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return &name[..pos];
            }
        }
        name
    }
}

/// Result of a dry-build parse: updates + optional download/unpacked sizes
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub updates: Vec<FlakeUpdate>,
    /// e.g. "1174.94 MiB" from "these 42 paths will be fetched (1174.94 MiB download, 4811.63 MiB unpacked):"
    pub download_size: String,
    /// e.g. "4811.63 MiB"
    pub unpacked_size: String,
}

/// Get the cache directory for a given flake path
fn get_cache_dir(flake_path: &Path) -> PathBuf {
    let cache_dir = directories::ProjectDirs::from("", "", "nixos-update-checker")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp/nixos-update-checker"));

    // Create a safe directory name from the flake path
    let safe_name = flake_path
        .to_string_lossy()
        .replace('/', "_")
        .replace('\\', "_")
        .trim_start_matches('_')
        .to_string();

    cache_dir.join("repos").join(safe_name)
}

/// Get the git remote URL by reading .git/config directly (avoids ownership issues)
fn get_git_remote(repo_path: &Path) -> Result<String, FlakeCheckError> {
    let git_config = repo_path.join(".git").join("config");

    if !git_config.exists() {
        return Err(FlakeCheckError::NotGitRepo(repo_path.display().to_string()));
    }

    let content = std::fs::read_to_string(&git_config).map_err(|e| {
        FlakeCheckError::NixError(format!("Failed to read git config: {}", e))
    })?;

    // Parse the git config to find remote.origin.url
    let mut in_remote_origin = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[remote \"origin\"]" {
            in_remote_origin = true;
        } else if trimmed.starts_with('[') {
            in_remote_origin = false;
        } else if in_remote_origin && trimmed.starts_with("url = ") {
            return Ok(trimmed.strip_prefix("url = ").unwrap().to_string());
        }
    }

    Err(FlakeCheckError::NixError("No origin remote found".to_string()))
}

/// Get the current branch by reading .git/HEAD directly
fn get_git_branch(repo_path: &Path) -> Result<String, FlakeCheckError> {
    let head_file = repo_path.join(".git").join("HEAD");

    if !head_file.exists() {
        return Ok("main".to_string());
    }

    let content = std::fs::read_to_string(&head_file).map_err(|_| {
        FlakeCheckError::NixError("Failed to read HEAD".to_string())
    })?;

    // HEAD contains "ref: refs/heads/branch-name" or a commit hash
    if let Some(branch) = content.trim().strip_prefix("ref: refs/heads/") {
        Ok(branch.to_string())
    } else {
        Ok("main".to_string()) // Detached HEAD, default to main
    }
}

/// Clone or update the cache repository
async fn sync_cache_repo(original_path: &Path, cache_path: &Path) -> Result<(), FlakeCheckError> {
    // Get the remote URL from the original repo (reads .git/config directly)
    let remote_url = get_git_remote(original_path)?;
    let branch = get_git_branch(original_path)?;

    eprintln!("Remote URL: {}, Branch: {}", remote_url, branch);

    if cache_path.join(".git").exists() {
        // Cache exists, fetch and reset to match remote
        eprintln!("Updating cache at {}", cache_path.display());

        let fetch = Command::new("git")
            .args(["fetch", "origin", &branch])
            .current_dir(cache_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !fetch.status.success() {
            let stderr = String::from_utf8_lossy(&fetch.stderr);
            return Err(FlakeCheckError::NixError(format!("Git fetch failed: {}", stderr)));
        }

        let reset = Command::new("git")
            .args(["reset", "--hard", &format!("origin/{}", branch)])
            .current_dir(cache_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !reset.status.success() {
            let stderr = String::from_utf8_lossy(&reset.stderr);
            return Err(FlakeCheckError::NixError(format!("Git reset failed: {}", stderr)));
        }
    } else {
        // Clone the repository
        eprintln!("Cloning {} to {}", remote_url, cache_path.display());

        // Create parent directory
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                FlakeCheckError::NixError(format!("Failed to create cache directory: {}", e))
            })?;
        }

        let clone = Command::new("git")
            .args(["clone", "--branch", &branch, "--single-branch", &remote_url])
            .arg(cache_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !clone.status.success() {
            let stderr = String::from_utf8_lossy(&clone.stderr);
            return Err(FlakeCheckError::NixError(format!("Git clone failed: {}", stderr)));
        }
    }

    // Copy the current flake.lock from the original to the cache
    // This ensures we're comparing against the current state
    let original_lock = original_path.join("flake.lock");
    let cache_lock = cache_path.join("flake.lock");

    if original_lock.exists() {
        std::fs::copy(&original_lock, &cache_lock).map_err(|e| {
            FlakeCheckError::NixError(format!("Failed to copy flake.lock: {}", e))
        })?;
    }

    Ok(())
}

/// Check for flake updates without actually applying them
/// We clone/sync the repo to a user-writable cache directory and check there
pub async fn check_for_updates(flake_path: &Path) -> Result<CheckResult, FlakeCheckError> {
    if !flake_path.exists() {
        return Err(FlakeCheckError::PathNotFound(
            flake_path.display().to_string(),
        ));
    }

    // Get or create the cache directory
    let cache_path = get_cache_dir(flake_path);

    // Sync the cache with the original repo
    sync_cache_repo(flake_path, &cache_path).await?;

    // Run nix flake update in the cache directory
    eprintln!("Running nix flake update in {}", cache_path.display());

    let update_output = Command::new("nix")
        .args(["flake", "update"])
        .current_dir(&cache_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    let update_stderr = String::from_utf8_lossy(&update_output.stderr);
    eprintln!("Nix flake update output:\n{}", update_stderr);

    eprintln!("Checking what packages would be rebuilt...");

    // Get hostname for the flake reference
    let hostname = gethostname();

    // Run nixos-rebuild dry-build to see what packages would change
    eprintln!("Running: nixos-rebuild dry-build --flake .#{}", hostname);

    let dry_build = Command::new("nixos-rebuild")
        .args([
            "dry-build",
            "--flake",
            &format!(".#{}", hostname),
        ])
        .current_dir(&cache_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&dry_build.stdout);
    let stderr = String::from_utf8_lossy(&dry_build.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    eprintln!("nixos-rebuild dry-build exit status: {:?}", dry_build.status);
    eprintln!("nixos-rebuild dry-build output:\n{}", combined);

    if !dry_build.status.success() {
        return Err(FlakeCheckError::NixError(format!(
            "nixos-rebuild dry-build failed: {}",
            stderr.lines().take(5).collect::<Vec<_>>().join("\n")
        )));
    }

    // Get current system packages to compare hashes
    eprintln!("Getting current system packages for comparison...");
    let current_packages = get_current_system_packages().await;
    eprintln!("Found {} current packages", current_packages.len());

    // Parse the output for packages that would be built
    parse_dry_build_output(&combined, &current_packages)
}

/// Get the system hostname
fn gethostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "nixos".to_string())
}

/// Package info from current system: (full_name, hash_short)
type CurrentPackageInfo = (String, String);

/// Get a map of package base names to their current info (full name + hash)
async fn get_current_system_packages() -> std::collections::HashMap<String, CurrentPackageInfo> {
    let mut packages: std::collections::HashMap<String, CurrentPackageInfo> = std::collections::HashMap::new();

    // Query the current system closure
    let output = Command::new("nix-store")
        .args(["-qR", "/run/current-system"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some((name, hash)) = extract_package_info(line.trim()) {
                // Use base name for matching
                let update = FlakeUpdate {
                    package_name: name.clone(),
                    old_package_name: String::new(),
                    hash_short: String::new(),
                    old_hash_short: String::new(),
                };
                let base = update.base_name().to_string();
                // Keep the one with version if there's a conflict
                let should_insert = match packages.get(&base) {
                    None => true,
                    Some((existing_name, _)) => name.len() > existing_name.len(),
                };
                if should_insert {
                    packages.insert(base, (name, hash));
                }
            }
        }
    }

    packages
}

/// Parse the output of `nixos-rebuild dry-build` to extract packages
fn parse_dry_build_output(output: &str, current_packages: &std::collections::HashMap<String, CurrentPackageInfo>) -> Result<CheckResult, FlakeCheckError> {
    let mut updates: Vec<FlakeUpdate> = Vec::new();
    let mut in_build_list = false;
    let mut download_size = String::new();
    let mut unpacked_size = String::new();

    // Match nix format: "these 42 paths will be fetched (1174.94 MiB download, 4811.63 MiB unpacked):"
    // Also handles singular: "this path will be fetched (0.02 MiB download, 0.06 MiB unpacked):"
    let size_re = Regex::new(r"will be fetched \(([\d.]+ \S+) download, ([\d.]+ \S+) unpacked\)").unwrap();

    for line in output.lines() {
        let trimmed = line.trim();

        // Look for "these derivations will be built:" or "these paths will be fetched:"
        if trimmed.contains("derivations will be built") || trimmed.contains("will be fetched") {
            // Extract download and unpacked sizes if present
            if let Some(caps) = size_re.captures(trimmed) {
                download_size = caps[1].to_string();
                unpacked_size = caps[2].to_string();
            }
            in_build_list = true;
            continue;
        }

        // End of list when we hit an empty line or different section
        if in_build_list && (trimmed.is_empty() || (!trimmed.starts_with("/nix/store/") && !trimmed.starts_with("  "))) {
            in_build_list = false;
        }

        // Parse derivation/store paths
        if in_build_list && trimmed.starts_with("/nix/store/") {
            // Extract package info from path like /nix/store/xxx-packagename-1.2.3.drv
            if let Some((name, hash)) = extract_package_info(trimmed) {
                // Look up old package info from current system
                let temp_update = FlakeUpdate {
                    package_name: name.clone(),
                    old_package_name: String::new(),
                    hash_short: String::new(),
                    old_hash_short: String::new(),
                };
                let base_name = temp_update.base_name();
                let (old_name, old_hash) = current_packages
                    .get(base_name)
                    .cloned()
                    .unwrap_or_default();

                let new_update = FlakeUpdate {
                    package_name: name.clone(),
                    old_package_name: old_name,
                    hash_short: hash,
                    old_hash_short: old_hash,
                };

                // Check for duplicates - prefer version with version number
                let dominated = updates.iter().any(|existing| {
                    // If existing has a version and new one doesn't, existing dominates
                    existing.base_name() == new_update.base_name()
                        && existing.package_name.len() > new_update.package_name.len()
                });

                if dominated {
                    continue;
                }

                // Remove any existing entry that the new one dominates
                updates.retain(|existing| {
                    !(existing.base_name() == new_update.base_name()
                        && existing.package_name.len() < new_update.package_name.len())
                });

                // Avoid exact duplicates
                if !updates.iter().any(|u| u.package_name == name) {
                    updates.push(new_update);
                }
            }
        }
    }

    // Sort by package name
    updates.sort_by(|a, b| a.package_name.to_lowercase().cmp(&b.package_name.to_lowercase()));

    Ok(CheckResult {
        updates,
        download_size,
        unpacked_size,
    })
}

/// Check if a package name is an internal/system package that should be filtered out
fn is_internal_package(name: &str) -> bool {
    // Exact matches
    let exact_matches = [
        "system-path",
        "user-units",
        "dbus-1",
        "unit",
        "etc",
    ];

    // Prefix matches
    let prefix_matches = [
        "X-Restart-Triggers",
        "unit-",
        "etc-",
        "system-units",
        "nixos-system-",
        "user-environment",
    ];

    // Suffix matches
    let suffix_matches = [
        ".service",
        ".socket",
        ".timer",
        ".target",
        ".mount",
    ];

    // Check exact matches
    if exact_matches.contains(&name) {
        return true;
    }

    // Check prefix matches
    for prefix in &prefix_matches {
        if name.starts_with(prefix) {
            return true;
        }
    }

    // Check suffix matches (systemd units)
    for suffix in &suffix_matches {
        if name.ends_with(suffix) {
            return true;
        }
    }

    false
}

/// Extract package info from a nix store path
/// /nix/store/hash-name-version.drv -> (name-version, hash_short)
/// /nix/store/hash-name-version -> (name-version, hash_short)
fn extract_package_info(path: &str) -> Option<(String, String)> {
    // Remove /nix/store/ prefix and .drv suffix
    let path = path.trim();
    let name = path.strip_prefix("/nix/store/")?;
    let name = name.strip_suffix(".drv").unwrap_or(name);

    // Skip the hash (32 chars + dash)
    if name.len() > 33 && name.chars().nth(32) == Some('-') {
        let hash_short = name[..8].to_string();
        let package_name = name[33..].to_string();

        // Filter out internal/system packages
        if is_internal_package(&package_name) {
            return None;
        }

        Some((package_name, hash_short))
    } else {
        None
    }
}

/// Generate a commit message from the list of updates
pub fn generate_commit_message(updates: &[FlakeUpdate]) -> String {
    if updates.is_empty() {
        return "Update flake inputs".to_string();
    }

    if updates.len() == 1 {
        return format!("Update {}", updates[0].package_name);
    }

    format!("Update {} packages", updates.len())
}

/// Format updates for tooltip display
pub fn format_updates_tooltip(updates: &[FlakeUpdate], download_size: &str, unpacked_size: &str) -> String {
    if updates.is_empty() {
        return "No updates available".to_string();
    }

    let size_info = if !download_size.is_empty() && !unpacked_size.is_empty() {
        format!(" ({} download, {} unpacked)", download_size, unpacked_size)
    } else if !download_size.is_empty() {
        format!(" ({} download)", download_size)
    } else {
        String::new()
    };

    let header = if updates.len() == 1 {
        format!("1 package to update{}:", size_info)
    } else {
        format!("{} packages to update{}:", updates.len(), size_info)
    };

    // Show first few packages, truncate if too many
    let max_show = 10;
    let details: Vec<String> = updates.iter().take(max_show).map(|u| u.display()).collect();

    if updates.len() > max_show {
        format!("{}\n{}  ...and {} more", header, details.join("\n  "), updates.len() - max_show)
    } else {
        format!("{}\n  {}", header, details.join("\n  "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_internal_package() {
        // Should be filtered
        assert!(is_internal_package("system-path"));
        assert!(is_internal_package("user-environment"));
        assert!(is_internal_package("user-units"));
        assert!(is_internal_package("dbus-1"));
        assert!(is_internal_package("X-Restart-Triggers"));
        assert!(is_internal_package("X-Restart-Triggers-Dbus"));
        assert!(is_internal_package("X-Restart-Triggers-polkit"));
        assert!(is_internal_package("unit-dbus.service"));
        assert!(is_internal_package("unit-polkit.service"));
        assert!(is_internal_package("etc-hosts"));
        assert!(is_internal_package("etc"));
        assert!(is_internal_package("nixos-system-nixos-25.11.20260117.72ac591"));

        // Should NOT be filtered
        assert!(!is_internal_package("firefox-120.0"));
        assert!(!is_internal_package("chromium-119.0"));
        assert!(!is_internal_package("nodejs-20.10.0"));
        assert!(!is_internal_package("glibc-2.38"));
        assert!(!is_internal_package("whisper-typer-0.1.0"));
    }

    #[test]
    fn test_parse_dry_build_output() {
        let output = r#"
these 3 derivations will be built:
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-firefox-120.0.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-chromium-119.0.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-nodejs-20.10.0.drv
these 1 paths will be fetched (500.00 MiB download, 1500.00 MiB unpacked):
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-glibc-2.38
"#;
        let current: std::collections::HashMap<String, CurrentPackageInfo> = std::collections::HashMap::new();
        let result = parse_dry_build_output(output, &current).unwrap();
        assert_eq!(result.updates.len(), 4);
        assert!(result.updates.iter().any(|u| u.package_name == "firefox-120.0"));
        assert!(result.updates.iter().any(|u| u.package_name == "chromium-119.0"));
        assert!(result.updates.iter().any(|u| u.package_name == "nodejs-20.10.0"));
        assert!(result.updates.iter().any(|u| u.package_name == "glibc-2.38"));
        assert_eq!(result.download_size, "500.00 MiB");
        assert_eq!(result.unpacked_size, "1500.00 MiB");
    }

    #[test]
    fn test_parse_dry_build_output_filters_internal() {
        let output = r#"
these derivations will be built:
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-firefox-120.0.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-system-path.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-X-Restart-Triggers-Dbus.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-unit-dbus.service.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-chromium-119.0.drv
"#;
        let current: std::collections::HashMap<String, CurrentPackageInfo> = std::collections::HashMap::new();
        let result = parse_dry_build_output(output, &current).unwrap();
        // Should only have firefox and chromium, not the internal packages
        assert_eq!(result.updates.len(), 2);
        assert!(result.updates.iter().any(|u| u.package_name == "firefox-120.0"));
        assert!(result.updates.iter().any(|u| u.package_name == "chromium-119.0"));
        assert_eq!(result.download_size, "");
    }

    #[test]
    fn test_extract_package_info() {
        let result = extract_package_info("/nix/store/abcdefghijklmnopqrstuvwxyz012345-firefox-120.0.drv");
        assert_eq!(result, Some(("firefox-120.0".to_string(), "abcdefgh".to_string())));

        let result = extract_package_info("/nix/store/abcdefghijklmnopqrstuvwxyz012345-glibc-2.38");
        assert_eq!(result, Some(("glibc-2.38".to_string(), "abcdefgh".to_string())));

        // Internal packages should return None
        assert_eq!(
            extract_package_info("/nix/store/abcdefghijklmnopqrstuvwxyz012345-system-path.drv"),
            None
        );
    }

    #[test]
    fn test_base_name() {
        let update = FlakeUpdate {
            package_name: "whisper-typer-0.1.0".to_string(),
            old_package_name: String::new(),
            hash_short: "abcd1234".to_string(),
            old_hash_short: String::new(),
        };
        assert_eq!(update.base_name(), "whisper-typer");

        let update = FlakeUpdate {
            package_name: "whisper-typer".to_string(),
            old_package_name: String::new(),
            hash_short: "abcd1234".to_string(),
            old_hash_short: String::new(),
        };
        assert_eq!(update.base_name(), "whisper-typer");
    }

    #[test]
    fn test_deduplication() {
        let output = r#"
these derivations will be built:
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-whisper-typer-0.1.0.drv
  /nix/store/xyzdefghijklmnopqrstuvwxyz012345-whisper-typer.drv
"#;
        let current: std::collections::HashMap<String, CurrentPackageInfo> = std::collections::HashMap::new();
        let result = parse_dry_build_output(output, &current).unwrap();
        // Should only have one entry, preferring the one with version
        assert_eq!(result.updates.len(), 1);
        assert_eq!(result.updates[0].package_name, "whisper-typer-0.1.0");
    }

    #[test]
    fn test_old_hash_lookup() {
        // Hash must be exactly 32 characters: newhashabcdefghijklmnopqrstuvwxy
        let output = r#"
these derivations will be built:
  /nix/store/newhashabcdefghijklmnopqrstuvwxy-firefox-120.0.drv
"#;
        let mut current: std::collections::HashMap<String, CurrentPackageInfo> = std::collections::HashMap::new();
        current.insert("firefox".to_string(), ("firefox-119.0".to_string(), "oldhas12".to_string()));

        let result = parse_dry_build_output(output, &current).unwrap();
        assert_eq!(result.updates.len(), 1);
        assert_eq!(result.updates[0].package_name, "firefox-120.0");
        assert_eq!(result.updates[0].old_package_name, "firefox-119.0");
        assert_eq!(result.updates[0].hash_short, "newhasha");
        assert_eq!(result.updates[0].old_hash_short, "oldhas12");
    }

    #[test]
    fn test_generate_commit_message() {
        let updates = vec![
            FlakeUpdate { package_name: "firefox-120.0".to_string(), old_package_name: String::new(), hash_short: "abcd1234".to_string(), old_hash_short: String::new() },
            FlakeUpdate { package_name: "chromium-119.0".to_string(), old_package_name: String::new(), hash_short: "efgh5678".to_string(), old_hash_short: String::new() },
        ];

        let msg = generate_commit_message(&updates);
        assert!(msg.contains("2 packages"));
    }

    #[test]
    fn test_version_display() {
        // Version upgrade
        let update = FlakeUpdate {
            package_name: "firefox-120.0".to_string(),
            old_package_name: "firefox-119.0".to_string(),
            hash_short: "newhash1".to_string(),
            old_hash_short: "oldhash1".to_string(),
        };
        assert_eq!(update.version(), Some("120.0"));
        assert_eq!(update.old_version(), Some("119.0"));
        assert_eq!(update.display(), "firefox (119.0 → 120.0)");

        // Same version, different hash
        let update = FlakeUpdate {
            package_name: "myapp-1.0.0".to_string(),
            old_package_name: "myapp-1.0.0".to_string(),
            hash_short: "newhash1".to_string(),
            old_hash_short: "oldhash1".to_string(),
        };
        assert_eq!(update.display(), "myapp (1.0.0 oldhash1 → newhash1)");

        // No version info, fall back to hashes
        let update = FlakeUpdate {
            package_name: "myapp".to_string(),
            old_package_name: "myapp".to_string(),
            hash_short: "newhash1".to_string(),
            old_hash_short: "oldhash1".to_string(),
        };
        assert_eq!(update.display(), "myapp (oldhash1 → newhash1)");
    }
}
