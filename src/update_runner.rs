use std::process::Command;
use thiserror::Error;

use crate::flake_checker::{generate_commit_message, FlakeUpdate};

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Failed to spawn terminal: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("Unsupported terminal: {0}")]
    UnsupportedTerminal(String),
}

/// Run the update process in a terminal
pub fn run_update(
    flake_path: &str,
    terminal: &str,
    updates: &[FlakeUpdate],
) -> Result<(), UpdateError> {
    let commit_msg = generate_commit_message(updates);

    // Build the update script
    let script = build_update_script(flake_path, &commit_msg);

    // Spawn the terminal with the script
    spawn_terminal(terminal, &script)?;

    Ok(())
}

/// Build the shell script that performs the update
fn build_update_script(flake_path: &str, commit_msg: &str) -> String {
    // Escape single quotes in commit message
    let escaped_msg = commit_msg.replace('\'', "'\\''");

    format!(
        r#"#!/bin/bash
set -e

echo "=== NixOS Flake Update ==="
echo ""

cd '{flake_path}'
echo "Working directory: $(pwd)"
echo ""

echo "=== Updating flake inputs ==="
nix flake update
echo ""

echo "=== Rebuilding NixOS ==="
sudo nixos-rebuild switch --flake .
echo ""

echo "=== Committing changes ==="
git add -A
git commit -m '{escaped_msg}' || echo "Nothing to commit"
echo ""

echo "=== Pushing to remote ==="
git push || echo "Push failed or no remote configured"
echo ""

echo "=== Update complete! ==="
echo ""
echo "Press Enter to close this terminal..."
read
"#,
        flake_path = flake_path,
        escaped_msg = escaped_msg,
    )
}

/// Spawn a terminal emulator with the given script
fn spawn_terminal(terminal: &str, script: &str) -> Result<(), UpdateError> {
    // Write script to a temporary file
    let script_path = std::env::temp_dir().join("nixos-update-script.sh");
    std::fs::write(&script_path, script)?;

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    let script_path_str = script_path.to_string_lossy().to_string();

    // Build command based on terminal type
    let mut cmd = match terminal.to_lowercase().as_str() {
        "ghostty" => {
            let mut c = Command::new("ghostty");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "konsole" => {
            let mut c = Command::new("konsole");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "gnome-terminal" => {
            let mut c = Command::new("gnome-terminal");
            c.arg("--").arg(&script_path_str);
            c
        }
        "alacritty" => {
            let mut c = Command::new("alacritty");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "kitty" => {
            let mut c = Command::new("kitty");
            c.arg(&script_path_str);
            c
        }
        "wezterm" => {
            let mut c = Command::new("wezterm");
            c.arg("start").arg("--").arg(&script_path_str);
            c
        }
        "xterm" => {
            let mut c = Command::new("xterm");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "foot" => {
            let mut c = Command::new("foot");
            c.arg(&script_path_str);
            c
        }
        "terminator" => {
            let mut c = Command::new("terminator");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "tilix" => {
            let mut c = Command::new("tilix");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "urxvt" | "rxvt-unicode" => {
            let mut c = Command::new("urxvt");
            c.arg("-e").arg(&script_path_str);
            c
        }
        "st" => {
            let mut c = Command::new("st");
            c.arg("-e").arg(&script_path_str);
            c
        }
        _ => {
            // Try to use the terminal name directly with -e flag
            let mut c = Command::new(terminal);
            c.arg("-e").arg(&script_path_str);
            c
        }
    };

    cmd.spawn()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_update_script() {
        let script = build_update_script("/home/user/nixos", "Update nixpkgs");
        assert!(script.contains("cd '/home/user/nixos'"));
        assert!(script.contains("nix flake update"));
        assert!(script.contains("nixos-rebuild switch"));
        assert!(script.contains("Update nixpkgs"));
    }

    #[test]
    fn test_commit_message_escaping() {
        let script = build_update_script("/path", "It's a test");
        assert!(script.contains("It'\\''s a test"));
    }
}
