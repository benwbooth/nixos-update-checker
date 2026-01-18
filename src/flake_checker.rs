use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
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
}

/// Represents a single flake input update
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeUpdate {
    pub input_name: String,
    pub old_rev: String,
    pub new_rev: String,
    pub old_date: Option<String>,
    pub new_date: Option<String>,
}

impl FlakeUpdate {
    /// Format the update for display in tooltip
    pub fn display(&self) -> String {
        let old = if let Some(date) = &self.old_date {
            format!("{} ({})", self.short_rev(&self.old_rev), date)
        } else {
            self.short_rev(&self.old_rev)
        };

        let new = if let Some(date) = &self.new_date {
            format!("{} ({})", self.short_rev(&self.new_rev), date)
        } else {
            self.short_rev(&self.new_rev)
        };

        format!("{}: {} → {}", self.input_name, old, new)
    }

    fn short_rev(&self, rev: &str) -> String {
        if rev.len() > 7 {
            rev[..7].to_string()
        } else {
            rev.to_string()
        }
    }
}

/// Check for flake updates without actually applying them
pub async fn check_for_updates(flake_path: &Path) -> Result<Vec<FlakeUpdate>, FlakeCheckError> {
    if !flake_path.exists() {
        return Err(FlakeCheckError::PathNotFound(
            flake_path.display().to_string(),
        ));
    }

    // Run nix flake update --dry-run
    let output = Command::new("nix")
        .args(["flake", "update", "--dry-run"])
        .current_dir(flake_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    // nix flake update --dry-run outputs to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Combine both outputs for parsing
    let combined = format!("{}\n{}", stdout, stderr);

    // Parse the output for updates
    parse_flake_update_output(&combined)
}

/// Parse the output of `nix flake update --dry-run` to extract updates
fn parse_flake_update_output(output: &str) -> Result<Vec<FlakeUpdate>, FlakeCheckError> {
    let mut updates = Vec::new();

    // Pattern for update lines like:
    // • Updated input 'nixpkgs':
    //     'github:NixOS/nixpkgs/abc123' (2024-01-01)
    //   → 'github:NixOS/nixpkgs/def456' (2024-01-15)
    //
    // Or simpler format:
    // • Updated input 'nixpkgs': 'github:...' -> 'github:...'

    // Regex for the multi-line format
    let update_header = Regex::new(r"[•\*]\s*Updated input '([^']+)'").unwrap();
    let ref_line = Regex::new(r"'([^']+/([a-f0-9]+))'(?:\s*\(([^)]+)\))?").unwrap();

    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if let Some(caps) = update_header.captures(line) {
            let input_name = caps.get(1).unwrap().as_str().to_string();

            // Look for old and new refs in subsequent lines or same line
            let mut old_rev = String::new();
            let mut new_rev = String::new();
            let mut old_date = None;
            let mut new_date = None;

            // Check if refs are on the same line
            let refs_on_line: Vec<_> = ref_line.captures_iter(line).collect();
            if refs_on_line.len() >= 2 {
                old_rev = refs_on_line[0].get(2).unwrap().as_str().to_string();
                old_date = refs_on_line[0].get(3).map(|m| m.as_str().to_string());
                new_rev = refs_on_line[1].get(2).unwrap().as_str().to_string();
                new_date = refs_on_line[1].get(3).map(|m| m.as_str().to_string());
            } else {
                // Look at subsequent lines
                for j in (i + 1)..std::cmp::min(i + 4, lines.len()) {
                    let next_line = lines[j];
                    if let Some(ref_caps) = ref_line.captures(next_line) {
                        let rev = ref_caps.get(2).unwrap().as_str().to_string();
                        let date = ref_caps.get(3).map(|m| m.as_str().to_string());

                        if next_line.contains('→') || next_line.contains("->") {
                            new_rev = rev;
                            new_date = date;
                        } else if old_rev.is_empty() {
                            old_rev = rev;
                            old_date = date;
                        } else if new_rev.is_empty() {
                            new_rev = rev;
                            new_date = date;
                        }
                    }
                }
            }

            if !old_rev.is_empty() && !new_rev.is_empty() && old_rev != new_rev {
                updates.push(FlakeUpdate {
                    input_name,
                    old_rev,
                    new_rev,
                    old_date,
                    new_date,
                });
            }
        }

        i += 1;
    }

    Ok(updates)
}

/// Generate a commit message from the list of updates
pub fn generate_commit_message(updates: &[FlakeUpdate]) -> String {
    if updates.is_empty() {
        return "Update flake inputs".to_string();
    }

    if updates.len() == 1 {
        return format!("Update {}", updates[0].input_name);
    }

    let names: Vec<&str> = updates.iter().map(|u| u.input_name.as_str()).collect();
    format!("Update {}", names.join(", "))
}

/// Format updates for tooltip display
pub fn format_updates_tooltip(updates: &[FlakeUpdate]) -> String {
    if updates.is_empty() {
        return "No updates available".to_string();
    }

    let header = if updates.len() == 1 {
        "1 update available:".to_string()
    } else {
        format!("{} updates available:", updates.len())
    };

    let details: Vec<String> = updates.iter().map(|u| u.display()).collect();

    format!("{}\n{}", header, details.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_update_output() {
        let output = r#"
warning: updating lock file '/home/user/nixos/flake.lock':
• Updated input 'nixpkgs':
    'github:NixOS/nixpkgs/abc1234567890' (2024-01-01)
  → 'github:NixOS/nixpkgs/def4567890123' (2024-01-15)
• Updated input 'home-manager':
    'github:nix-community/home-manager/111222333' (2024-01-02)
  → 'github:nix-community/home-manager/444555666' (2024-01-16)
"#;

        let updates = parse_flake_update_output(output).unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].input_name, "nixpkgs");
        assert_eq!(updates[1].input_name, "home-manager");
    }

    #[test]
    fn test_generate_commit_message() {
        let updates = vec![
            FlakeUpdate {
                input_name: "nixpkgs".to_string(),
                old_rev: "abc".to_string(),
                new_rev: "def".to_string(),
                old_date: None,
                new_date: None,
            },
            FlakeUpdate {
                input_name: "home-manager".to_string(),
                old_rev: "111".to_string(),
                new_rev: "222".to_string(),
                old_date: None,
                new_date: None,
            },
        ];

        let msg = generate_commit_message(&updates);
        assert!(msg.contains("nixpkgs"));
        assert!(msg.contains("home-manager"));
    }
}
