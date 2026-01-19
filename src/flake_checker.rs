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
}

impl FlakeUpdate {
    /// Format the update for display in tooltip
    pub fn display(&self) -> String {
        self.package_name.clone()
    }
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
pub async fn check_for_updates(flake_path: &Path) -> Result<Vec<FlakeUpdate>, FlakeCheckError> {
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

    // Parse the output for packages that would be built
    parse_dry_build_output(&combined)
}

/// Get the system hostname
fn gethostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "nixos".to_string())
}

/// Parse the output of `nixos-rebuild dry-build` to extract packages
fn parse_dry_build_output(output: &str) -> Result<Vec<FlakeUpdate>, FlakeCheckError> {
    let mut updates = Vec::new();
    let mut in_build_list = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Look for "these derivations will be built:" or "these paths will be fetched:"
        if trimmed.contains("derivations will be built") || trimmed.contains("paths will be fetched") {
            in_build_list = true;
            continue;
        }

        // End of list when we hit an empty line or different section
        if in_build_list && (trimmed.is_empty() || (!trimmed.starts_with("/nix/store/") && !trimmed.starts_with("  "))) {
            in_build_list = false;
        }

        // Parse derivation/store paths
        if in_build_list && trimmed.starts_with("/nix/store/") {
            // Extract package name from path like /nix/store/xxx-packagename-1.2.3.drv
            if let Some(name) = extract_package_name(trimmed) {
                // Avoid duplicates
                if !updates.iter().any(|u: &FlakeUpdate| u.package_name == name) {
                    updates.push(FlakeUpdate { package_name: name });
                }
            }
        }
    }

    Ok(updates)
}

/// Extract package name from a nix store path
/// /nix/store/hash-name-version.drv -> name-version
/// /nix/store/hash-name-version -> name-version
fn extract_package_name(path: &str) -> Option<String> {
    // Remove /nix/store/ prefix and .drv suffix
    let path = path.trim();
    let name = path.strip_prefix("/nix/store/")?;
    let name = name.strip_suffix(".drv").unwrap_or(name);

    // Skip the hash (32 chars + dash)
    if name.len() > 33 && name.chars().nth(32) == Some('-') {
        Some(name[33..].to_string())
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
pub fn format_updates_tooltip(updates: &[FlakeUpdate]) -> String {
    if updates.is_empty() {
        return "No updates available".to_string();
    }

    let header = if updates.len() == 1 {
        "1 package to update:".to_string()
    } else {
        format!("{} packages to update:", updates.len())
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
    fn test_parse_dry_build_output() {
        let output = r#"
these 3 derivations will be built:
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-firefox-120.0.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-chromium-119.0.drv
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-nodejs-20.10.0.drv
these paths will be fetched (500 MiB):
  /nix/store/abcdefghijklmnopqrstuvwxyz012345-glibc-2.38
"#;

        let updates = parse_dry_build_output(output).unwrap();
        assert_eq!(updates.len(), 4);
        assert!(updates.iter().any(|u| u.package_name == "firefox-120.0"));
        assert!(updates.iter().any(|u| u.package_name == "chromium-119.0"));
        assert!(updates.iter().any(|u| u.package_name == "nodejs-20.10.0"));
        assert!(updates.iter().any(|u| u.package_name == "glibc-2.38"));
    }

    #[test]
    fn test_extract_package_name() {
        assert_eq!(
            extract_package_name("/nix/store/abcdefghijklmnopqrstuvwxyz012345-firefox-120.0.drv"),
            Some("firefox-120.0".to_string())
        );
        assert_eq!(
            extract_package_name("/nix/store/abcdefghijklmnopqrstuvwxyz012345-glibc-2.38"),
            Some("glibc-2.38".to_string())
        );
    }

    #[test]
    fn test_generate_commit_message() {
        let updates = vec![
            FlakeUpdate { package_name: "firefox-120.0".to_string() },
            FlakeUpdate { package_name: "chromium-119.0".to_string() },
        ];

        let msg = generate_commit_message(&updates);
        assert!(msg.contains("2 packages"));
    }
}
