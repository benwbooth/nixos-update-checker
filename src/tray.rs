#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, has_updates)]
        #[qproperty(i32, update_count)]
        #[qproperty(QString, update_summary)]
        #[qproperty(QString, tooltip_text)]
        #[qproperty(QString, flake_path)]
        #[qproperty(i32, check_interval)]
        #[qproperty(bool, checking)]
        #[qproperty(QString, last_check_time)]
        #[qproperty(QString, status_message)]
        // Store updates as a JSON string for simplicity
        #[qproperty(QString, updates_json)]
        // Update process output
        #[qproperty(bool, update_running)]
        #[qproperty(QString, update_output)]
        #[qproperty(QString, update_status_line)]
        type UpdateChecker = super::UpdateCheckerRust;

        /// Trigger a manual update check
        #[qinvokable]
        fn check_now(self: Pin<&mut UpdateChecker>);

        /// Run the actual update (embedded terminal)
        #[qinvokable]
        fn run_update(self: Pin<&mut UpdateChecker>);

        /// Save configuration
        #[qinvokable]
        fn save_config(self: Pin<&mut UpdateChecker>, flake_path: QString, interval: i32);

        /// Load configuration
        #[qinvokable]
        fn load_config(self: Pin<&mut UpdateChecker>);

        /// Get the icon path based on update state
        #[qinvokable]
        fn get_icon_path(self: &UpdateChecker) -> QString;

        /// Quit the application
        #[qinvokable]
        fn quit_app(self: &UpdateChecker);

        /// Clear the update output
        #[qinvokable]
        fn clear_output(self: Pin<&mut UpdateChecker>);
    }

    unsafe extern "RustQt" {
        /// Signal emitted when updates are found
        #[qsignal]
        fn updates_changed(self: Pin<&mut UpdateChecker>);

        /// Signal emitted when check starts/completes
        #[qsignal]
        fn check_status_changed(self: Pin<&mut UpdateChecker>);

        /// Signal emitted when config is loaded
        #[qsignal]
        fn config_loaded(self: Pin<&mut UpdateChecker>);

        /// Signal emitted when config is saved
        #[qsignal]
        fn config_saved(self: Pin<&mut UpdateChecker>);

        /// Signal emitted when update output changes
        #[qsignal]
        fn output_changed(self: Pin<&mut UpdateChecker>);

        /// Signal emitted when update process completes
        #[qsignal]
        fn update_completed(self: Pin<&mut UpdateChecker>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use std::sync::{Arc, Mutex};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use crate::config::Config;
use crate::flake_checker::{self, FlakeUpdate};

/// Rust implementation of the UpdateChecker QObject
#[derive(Default)]
pub struct UpdateCheckerRust {
    has_updates: bool,
    update_count: i32,
    update_summary: QString,
    tooltip_text: QString,
    flake_path: QString,
    check_interval: i32,
    checking: bool,
    last_check_time: QString,
    status_message: QString,
    updates_json: QString,
    update_running: bool,
    update_output: QString,
    update_status_line: QString,
}

/// Serialize updates to JSON string
fn updates_to_json(updates: &[FlakeUpdate]) -> String {
    serde_json::to_string(updates).unwrap_or_else(|_| "[]".to_string())
}

/// Deserialize updates from JSON string
fn updates_from_json(json: &str) -> Vec<FlakeUpdate> {
    serde_json::from_str(json).unwrap_or_default()
}

impl qobject::UpdateChecker {
    /// Perform an update check
    pub fn check_now(mut self: Pin<&mut Self>) {
        if *self.as_ref().checking() {
            return;
        }

        let flake_path = self.as_ref().flake_path().to_string();
        if flake_path.is_empty() {
            self.as_mut().set_status_message(QString::from("No flake path configured"));
            return;
        }

        self.as_mut().set_checking(true);
        self.as_mut().set_status_message(QString::from("Checking for updates..."));
        self.as_mut().check_status_changed();

        // Run check synchronously
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(flake_checker::check_for_updates(
            &std::path::PathBuf::from(&flake_path),
        ));

        match result {
            Ok(updates) => {
                let has_updates = !updates.is_empty();
                let count = updates.len() as i32;
                let summary = QString::from(&flake_checker::format_updates_tooltip(&updates));
                let tooltip = if updates.is_empty() {
                    QString::from("NixOS Update Checker\nNo updates available")
                } else {
                    QString::from(&format!(
                        "NixOS Update Checker\n{}",
                        flake_checker::format_updates_tooltip(&updates)
                    ))
                };
                let json = QString::from(&updates_to_json(&updates));

                self.as_mut().set_has_updates(has_updates);
                self.as_mut().set_update_count(count);
                self.as_mut().set_update_summary(summary);
                self.as_mut().set_tooltip_text(tooltip);
                self.as_mut().set_updates_json(json);
                self.as_mut().set_status_message(QString::from("Check complete"));
            }
            Err(e) => {
                self.as_mut().set_status_message(QString::from(&format!("Error: {}", e)));
            }
        }

        // Update last check time
        let now = chrono::Local::now();
        self.as_mut().set_last_check_time(QString::from(
            &now.format("%H:%M:%S").to_string(),
        ));
        self.as_mut().set_checking(false);
        self.as_mut().check_status_changed();
        self.as_mut().updates_changed();
    }

    pub fn run_update(mut self: Pin<&mut Self>) {
        if *self.as_ref().update_running() {
            return;
        }

        let flake_path = self.flake_path().to_string();
        let updates_json = self.updates_json().to_string();

        if flake_path.is_empty() {
            return;
        }

        // Parse updates from JSON
        let updates = updates_from_json(&updates_json);
        let commit_msg = flake_checker::generate_commit_message(&updates);

        // Clear previous output and mark as running
        self.as_mut().set_update_output(QString::from(""));
        self.as_mut().set_update_status_line(QString::from("Starting update..."));
        self.as_mut().set_update_running(true);
        self.as_mut().output_changed();

        // Build the update script
        let script = build_update_script(&flake_path, &commit_msg);

        // Write script to temp file
        let script_path = std::env::temp_dir().join("nixos-update-script.sh");
        if let Err(e) = std::fs::write(&script_path, &script) {
            self.as_mut().set_update_status_line(QString::from(&format!("Failed to write script: {}", e)));
            self.as_mut().set_update_running(false);
            self.as_mut().output_changed();
            return;
        }

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&script_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&script_path, perms);
            }
        }

        // Run pkexec to get sudo privileges, then run the script
        // We use pkexec which provides a graphical sudo prompt
        let mut child = match Command::new("pkexec")
            .arg("bash")
            .arg(&script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                self.as_mut().set_update_status_line(QString::from(&format!("Failed to start: {}", e)));
                self.as_mut().set_update_running(false);
                self.as_mut().output_changed();
                return;
            }
        };

        // Read output in a blocking way (we're already in a simple context)
        // For a proper async solution, we'd need channels, but this works for now
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let output = Arc::new(Mutex::new(String::new()));
        let output_clone = output.clone();

        // Collect all output
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let mut out = output_clone.lock().unwrap();
                out.push_str(&line);
                out.push('\n');
            }
        }

        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let mut out = output_clone.lock().unwrap();
                out.push_str(&line);
                out.push('\n');
            }
        }

        let status = child.wait();
        let final_output = output.lock().unwrap().clone();

        // Get the last non-empty line as status
        let last_line = final_output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .unwrap_or("Update complete")
            .to_string();

        self.as_mut().set_update_output(QString::from(&final_output));
        self.as_mut().set_update_status_line(QString::from(&last_line));
        self.as_mut().set_update_running(false);

        match status {
            Ok(s) if s.success() => {
                self.as_mut().set_status_message(QString::from("Update completed successfully"));
                // Clear the updates since we just applied them
                self.as_mut().set_has_updates(false);
                self.as_mut().set_update_count(0);
                self.as_mut().set_updates_json(QString::from("[]"));
            }
            Ok(s) => {
                self.as_mut().set_status_message(QString::from(&format!("Update failed (exit code: {:?})", s.code())));
            }
            Err(e) => {
                self.as_mut().set_status_message(QString::from(&format!("Update error: {}", e)));
            }
        }

        self.as_mut().output_changed();
        self.as_mut().update_completed();
        self.as_mut().updates_changed();
    }

    pub fn save_config(mut self: Pin<&mut Self>, flake_path: QString, interval: i32) {
        let config = Config {
            flake_path: flake_path.to_string(),
            check_interval_minutes: interval as u32,
        };

        if let Err(e) = config.save() {
            self.as_mut().set_status_message(QString::from(&format!("Failed to save config: {}", e)));
            return;
        }

        self.as_mut().set_flake_path(flake_path);
        self.as_mut().set_check_interval(interval);
        self.as_mut().set_status_message(QString::from("Configuration saved"));
        self.as_mut().config_saved();
    }

    pub fn load_config(mut self: Pin<&mut Self>) {
        match Config::load() {
            Ok(config) => {
                self.as_mut().set_flake_path(QString::from(&config.flake_path));
                self.as_mut().set_check_interval(config.check_interval_minutes as i32);
                self.as_mut().set_status_message(QString::from("Configuration loaded"));
                self.as_mut().config_loaded();
            }
            Err(e) => {
                self.as_mut().set_status_message(QString::from(&format!("Failed to load config: {}", e)));
                // Set defaults
                self.as_mut().set_flake_path(QString::from("/etc/nixos"));
                self.as_mut().set_check_interval(60);
                self.as_mut().config_loaded();
            }
        }
    }

    pub fn get_icon_path(&self) -> QString {
        if *self.has_updates() {
            QString::from("qrc:/icons/nix-flake-update.svg")
        } else {
            QString::from("qrc:/icons/nix-flake.svg")
        }
    }

    pub fn quit_app(&self) {
        std::process::exit(0);
    }

    pub fn clear_output(mut self: Pin<&mut Self>) {
        self.as_mut().set_update_output(QString::from(""));
        self.as_mut().set_update_status_line(QString::from(""));
        self.as_mut().output_changed();
    }
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
nixos-rebuild switch --flake .
echo ""

echo "=== Committing changes ==="
# Run git as the original user, not as root
REAL_USER="${{SUDO_USER:-$USER}}"
if [ -n "$REAL_USER" ] && [ "$REAL_USER" != "root" ]; then
    su - "$REAL_USER" -c "cd '{flake_path}' && git add -A && git commit -m '{escaped_msg}'" || echo "Nothing to commit"
    echo ""
    echo "=== Pushing to remote ==="
    su - "$REAL_USER" -c "cd '{flake_path}' && git push" || echo "Push failed or no remote configured"
else
    git add -A
    git commit -m '{escaped_msg}' || echo "Nothing to commit"
    echo ""
    echo "=== Pushing to remote ==="
    git push || echo "Push failed or no remote configured"
fi
echo ""

echo "=== Update complete! ==="
"#,
        flake_path = flake_path,
        escaped_msg = escaped_msg,
    )
}
