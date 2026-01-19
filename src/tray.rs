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
        #[qproperty(QString, check_interval_unit)]  // "hours", "days", "weeks"
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

        /// Trigger a manual update check (async - spawns background thread)
        #[qinvokable]
        fn check_now(self: Pin<&mut UpdateChecker>);

        /// Poll for check result (called by QML timer)
        #[qinvokable]
        fn poll_check_result(self: Pin<&mut UpdateChecker>);

        /// Run the actual update (async - spawns background thread)
        #[qinvokable]
        fn run_update(self: Pin<&mut UpdateChecker>);

        /// Poll for update progress (called by QML timer)
        #[qinvokable]
        fn poll_update_result(self: Pin<&mut UpdateChecker>);

        /// Save configuration
        #[qinvokable]
        fn save_config(self: Pin<&mut UpdateChecker>, flake_path: QString, interval: i32, unit: QString);

        /// Load configuration
        #[qinvokable]
        fn load_config(self: Pin<&mut UpdateChecker>);

        /// Get the icon path based on update state
        #[qinvokable]
        fn get_icon_path(self: &UpdateChecker) -> QString;

        /// Check if an update check is due based on last check time and interval
        #[qinvokable]
        fn is_check_due(self: &UpdateChecker) -> bool;

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

use crate::config::{Config, IntervalUnit};
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
    check_interval_unit: QString,  // "hours", "days", "weeks"
    checking: bool,
    last_check_time: QString,
    status_message: QString,
    updates_json: QString,
    update_running: bool,
    update_output: QString,
    update_status_line: QString,
    // Internal: cached config for is_check_due
    #[allow(dead_code)]
    cached_last_check_timestamp: i64,
}

/// Serialize updates to JSON string
fn updates_to_json(updates: &[FlakeUpdate]) -> String {
    serde_json::to_string(updates).unwrap_or_else(|_| "[]".to_string())
}

/// Deserialize updates from JSON string
fn updates_from_json(json: &str) -> Vec<FlakeUpdate> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Convert ANSI escape codes to HTML for display in QML rich text
fn ansi_to_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    result.push_str("<pre style=\"font-family: monospace; white-space: pre-wrap;\">");

    let mut chars = s.chars().peekable();
    let mut current_color: Option<&str> = None;
    let mut is_bold = false;

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Parse the escape sequence
                let mut code = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() {
                        chars.next(); // consume the letter
                        break;
                    }
                    code.push(chars.next().unwrap());
                }

                // Close previous span if needed
                if current_color.is_some() || is_bold {
                    result.push_str("</span>");
                }

                // Parse color codes
                let (new_color, new_bold) = parse_ansi_code(&code);
                current_color = new_color;
                is_bold = new_bold || is_bold && new_color.is_none();

                // Open new span if needed
                if current_color.is_some() || is_bold {
                    result.push_str("<span style=\"");
                    if let Some(color) = current_color {
                        result.push_str("color: ");
                        result.push_str(color);
                        result.push_str("; ");
                    }
                    if is_bold {
                        result.push_str("font-weight: bold; ");
                    }
                    result.push_str("\">");
                }
            }
        } else if c == '<' {
            result.push_str("&lt;");
        } else if c == '>' {
            result.push_str("&gt;");
        } else if c == '&' {
            result.push_str("&amp;");
        } else if c == '\n' {
            result.push_str("<br/>");
        } else {
            result.push(c);
        }
    }

    if current_color.is_some() || is_bold {
        result.push_str("</span>");
    }
    result.push_str("</pre>");
    result
}

/// Parse ANSI code and return (color, is_bold)
fn parse_ansi_code(code: &str) -> (Option<&'static str>, bool) {
    let mut color = None;
    let mut bold = false;

    for part in code.split(';') {
        match part {
            "0" => { color = None; bold = false; } // Reset
            "1" => bold = true,
            "30" => color = Some("#000000"), // Black
            "31" => color = Some("#cc0000"), // Red
            "32" => color = Some("#00cc00"), // Green
            "33" => color = Some("#cccc00"), // Yellow
            "34" => color = Some("#0066cc"), // Blue
            "35" => color = Some("#cc00cc"), // Magenta
            "36" => color = Some("#00cccc"), // Cyan
            "37" => color = Some("#cccccc"), // White
            "0;31" => color = Some("#cc0000"),
            "0;32" => color = Some("#00cc00"),
            "0;33" => color = Some("#cccc00"),
            "0;34" => color = Some("#0066cc"),
            "0;35" => color = Some("#cc00cc"),
            "0;36" => color = Some("#00cccc"),
            "1;33" => { color = Some("#ffff00"); bold = true; } // Bright yellow
            _ => {}
        }
    }
    (color, bold)
}

/// Format a timestamp as relative time ("2 hours ago", "3 days ago", etc.)
fn format_relative_time(timestamp: i64) -> String {
    if timestamp == 0 {
        return "Never".to_string();
    }

    if let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0) {
        // Use HumanTime with the DateTime directly - it will show "X ago" for past times
        chrono_humanize::HumanTime::from(dt).to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Build tooltip text with last check time and update status
fn build_tooltip(updates: &[FlakeUpdate], last_check_timestamp: i64) -> String {
    let last_checked = format_relative_time(last_check_timestamp);

    if updates.is_empty() {
        format!(
            "NixOS Update Checker\nNo updates available\nLast checked: {}",
            last_checked
        )
    } else {
        format!(
            "NixOS Update Checker\n{}\nLast checked: {}",
            flake_checker::format_updates_tooltip(updates),
            last_checked
        )
    }
}

use once_cell::sync::Lazy;

/// Result from background check thread
type CheckResult = Result<Vec<FlakeUpdate>, flake_checker::FlakeCheckError>;

/// Global storage for background check result
static CHECK_RESULT: Lazy<Arc<Mutex<Option<CheckResult>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

/// State for async update process
#[derive(Default)]
struct UpdateState {
    /// New output lines to append (raw with ANSI codes)
    new_output: String,
    /// Full accumulated raw output (for HTML conversion)
    full_output: String,
    /// Current status line
    status_line: String,
    /// Whether the process has completed
    completed: bool,
    /// Exit status (success/failure message)
    exit_status: Option<Result<(), String>>,
}

/// Global storage for background update state
static UPDATE_STATE: Lazy<Arc<Mutex<UpdateState>>> = Lazy::new(|| Arc::new(Mutex::new(UpdateState::default())));

impl qobject::UpdateChecker {
    /// Perform an update check (async - spawns background thread)
    pub fn check_now(mut self: Pin<&mut Self>) {
        if *self.as_ref().checking() {
            return;
        }

        let flake_path = self.as_ref().flake_path().to_string();
        if flake_path.is_empty() {
            self.as_mut().set_status_message(QString::from("No flake path configured"));
            return;
        }

        // Clear any previous result
        if let Ok(mut result) = CHECK_RESULT.lock() {
            *result = None;
        }

        self.as_mut().set_checking(true);
        self.as_mut().set_status_message(QString::from("Checking for updates..."));
        self.as_mut().check_status_changed();

        // Spawn background thread for the check
        let flake_path_clone = flake_path.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(flake_checker::check_for_updates(
                &std::path::PathBuf::from(&flake_path_clone),
            ));

            // Store result for main thread to pick up
            if let Ok(mut stored) = CHECK_RESULT.lock() {
                *stored = Some(result);
            }
        });
    }

    /// Poll for check completion (called by QML timer)
    pub fn poll_check_result(mut self: Pin<&mut Self>) {
        if !*self.as_ref().checking() {
            return;
        }

        // Check if result is available
        let result = {
            let mut stored = match CHECK_RESULT.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            stored.take()
        };

        if let Some(result) = result {
            // Process the result
            let check_timestamp = chrono::Utc::now().timestamp();

            match result {
                Ok(updates) => {
                    let has_updates = !updates.is_empty();
                    let count = updates.len() as i32;
                    let summary = QString::from(&flake_checker::format_updates_tooltip(&updates));
                    let tooltip = QString::from(&build_tooltip(&updates, check_timestamp));
                    let json = updates_to_json(&updates);

                    self.as_mut().set_has_updates(has_updates);
                    self.as_mut().set_update_count(count);
                    self.as_mut().set_update_summary(summary);
                    self.as_mut().set_tooltip_text(tooltip);
                    self.as_mut().set_updates_json(QString::from(&json));
                    self.as_mut().set_status_message(QString::from("Check complete"));

                    // Save to config for persistence
                    if let Ok(mut config) = Config::load() {
                        config.last_check_timestamp = check_timestamp;
                        config.cached_updates_json = json;
                        let _ = config.save();
                    }
                }
                Err(e) => {
                    self.as_mut().set_status_message(QString::from(&format!("Error: {}", e)));
                    let tooltip = QString::from(&format!(
                        "NixOS Update Checker\nCheck failed\nLast checked: {}",
                        format_relative_time(check_timestamp)
                    ));
                    self.as_mut().set_tooltip_text(tooltip);

                    // Still save timestamp
                    if let Ok(mut config) = Config::load() {
                        config.last_check_timestamp = check_timestamp;
                        let _ = config.save();
                    }
                }
            }

            // Update last check time (display)
            let now = chrono::Local::now();
            self.as_mut().set_last_check_time(QString::from(
                &now.format("%Y-%m-%d %H:%M:%S").to_string(),
            ));

            self.as_mut().set_checking(false);
            self.as_mut().check_status_changed();
            self.as_mut().updates_changed();
        }
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

        // Clear the update state
        if let Ok(mut state) = UPDATE_STATE.lock() {
            *state = UpdateState::default();
            state.status_line = "Starting update...".to_string();
        }

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

        // Spawn background thread to run the update
        let script_path_str = script_path.to_string_lossy().to_string();
        std::thread::spawn(move || {
            run_update_background(&script_path_str);
        });
    }

    /// Poll for update progress (called by QML timer)
    pub fn poll_update_result(mut self: Pin<&mut Self>) {
        if !*self.as_ref().update_running() {
            return;
        }

        // Check for new output and status
        let (has_new_output, full_output, status_line, completed, exit_status) = {
            let mut state = match UPDATE_STATE.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            let has_new = !state.new_output.is_empty();
            if has_new {
                let new_output = std::mem::take(&mut state.new_output);
                state.full_output.push_str(&new_output);
            }
            let full = state.full_output.clone();
            let status = state.status_line.clone();
            let done = state.completed;
            let exit = state.exit_status.take();
            (has_new, full, status, done, exit)
        };

        // Update output if there's new content
        if has_new_output {
            // Convert full raw output to HTML for rich text display with colors
            let html = ansi_to_html(&full_output);
            self.as_mut().set_update_output(QString::from(&html));
            self.as_mut().output_changed();
        }

        // Update status line
        if !status_line.is_empty() {
            self.as_mut().set_update_status_line(QString::from(&status_line));
        }

        // Handle completion
        if completed {
            self.as_mut().set_update_running(false);

            match exit_status {
                Some(Ok(())) => {
                    self.as_mut().set_status_message(QString::from("Update completed successfully"));
                    // Clear the updates since we just applied them
                    self.as_mut().set_has_updates(false);
                    self.as_mut().set_update_count(0);
                    self.as_mut().set_updates_json(QString::from("[]"));

                    // Also clear cached updates in config
                    if let Ok(mut config) = Config::load() {
                        config.cached_updates_json = String::new();
                        let _ = config.save();
                    }
                }
                Some(Err(msg)) => {
                    self.as_mut().set_status_message(QString::from(&msg));
                }
                None => {
                    self.as_mut().set_status_message(QString::from("Update completed"));
                }
            }

            self.as_mut().output_changed();
            self.as_mut().update_completed();
            self.as_mut().updates_changed();
        }
    }

    pub fn save_config(mut self: Pin<&mut Self>, flake_path: QString, interval: i32, unit: QString) {
        // Load existing config to preserve last_check_timestamp
        let existing = Config::load().unwrap_or_default();

        let config = Config {
            flake_path: flake_path.to_string(),
            check_interval: interval as u32,
            check_interval_unit: IntervalUnit::from_str(&unit.to_string()),
            last_check_timestamp: existing.last_check_timestamp,
            cached_updates_json: existing.cached_updates_json,
        };

        if let Err(e) = config.save() {
            self.as_mut().set_status_message(QString::from(&format!("Failed to save config: {}", e)));
            return;
        }

        self.as_mut().set_flake_path(flake_path);
        self.as_mut().set_check_interval(interval);
        self.as_mut().set_check_interval_unit(unit);
        self.as_mut().set_status_message(QString::from("Configuration saved"));
        self.as_mut().config_saved();
    }

    pub fn load_config(mut self: Pin<&mut Self>) {
        match Config::load() {
            Ok(config) => {
                self.as_mut().set_flake_path(QString::from(&config.flake_path));
                self.as_mut().set_check_interval(config.check_interval as i32);
                self.as_mut().set_check_interval_unit(QString::from(config.check_interval_unit.as_str()));

                // Format last check time for display
                if config.last_check_timestamp > 0 {
                    if let Some(dt) = chrono::DateTime::from_timestamp(config.last_check_timestamp, 0) {
                        let local: chrono::DateTime<chrono::Local> = dt.into();
                        self.as_mut().set_last_check_time(QString::from(
                            &local.format("%Y-%m-%d %H:%M:%S").to_string(),
                        ));
                    }
                }

                // Restore cached updates from last check
                let updates = updates_from_json(&config.cached_updates_json);
                let has_updates = !updates.is_empty();
                let count = updates.len() as i32;

                self.as_mut().set_has_updates(has_updates);
                self.as_mut().set_update_count(count);
                self.as_mut().set_updates_json(QString::from(&config.cached_updates_json));

                if has_updates {
                    let summary = QString::from(&flake_checker::format_updates_tooltip(&updates));
                    self.as_mut().set_update_summary(summary);
                }

                // Set initial tooltip with cached status
                let tooltip = QString::from(&build_tooltip(&updates, config.last_check_timestamp));
                self.as_mut().set_tooltip_text(tooltip);

                self.as_mut().set_status_message(QString::from("Configuration loaded"));

                // Emit signal to update icon based on cached state
                if has_updates {
                    self.as_mut().updates_changed();
                }

                self.as_mut().config_loaded();
            }
            Err(e) => {
                self.as_mut().set_status_message(QString::from(&format!("Failed to load config: {}", e)));
                // Set defaults
                self.as_mut().set_flake_path(QString::from("/etc/nixos"));
                self.as_mut().set_check_interval(1);
                self.as_mut().set_check_interval_unit(QString::from("hours"));
                self.as_mut().set_tooltip_text(QString::from("NixOS Update Checker\nLast checked: Never"));
                self.as_mut().config_loaded();
            }
        }
    }

    pub fn is_check_due(&self) -> bool {
        match Config::load() {
            Ok(config) => config.is_check_due(),
            Err(_) => true, // If we can't load config, assume check is due
        }
    }

    pub fn get_icon_path(&self) -> QString {
        if *self.checking() {
            QString::from("qrc:/icons/nix-flake-checking.svg")
        } else if *self.has_updates() {
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

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${{BOLD}}${{CYAN}}╔══════════════════════════════════════╗${{NC}}"
echo -e "${{BOLD}}${{CYAN}}║       NixOS Flake Update             ║${{NC}}"
echo -e "${{BOLD}}${{CYAN}}╚══════════════════════════════════════╝${{NC}}"
echo ""

cd '{flake_path}'
echo -e "${{BLUE}}📁 Working directory:${{NC}} $(pwd)"
echo ""

# Add flake path to git safe directories (needed even as root in newer git)
git config --global --add safe.directory '{flake_path}'

echo -e "${{BOLD}}${{YELLOW}}━━━ Updating flake inputs ━━━${{NC}}"
# Use script to create a pseudo-TTY so nix outputs colors
script -qec "NIX_CONFIG='extra-experimental-features = nix-command flakes' nix flake update --log-format bar-with-logs" /dev/null
echo ""

echo -e "${{BOLD}}${{MAGENTA}}━━━ Rebuilding NixOS ━━━${{NC}}"
# Use script to create a pseudo-TTY so nix outputs colors
script -qec "nixos-rebuild switch --flake . --log-format bar-with-logs" /dev/null
echo ""

echo -e "${{BOLD}}${{BLUE}}━━━ Committing changes ━━━${{NC}}"
# Run git as the original user, not as root
REAL_USER="${{SUDO_USER:-$USER}}"
if [ -n "$REAL_USER" ] && [ "$REAL_USER" != "root" ]; then
    su - "$REAL_USER" -c "cd '{flake_path}' && git add -A && git commit -m '{escaped_msg}'" || echo -e "${{YELLOW}}Nothing to commit${{NC}}"
    echo ""
    echo -e "${{BOLD}}${{CYAN}}━━━ Pushing to remote ━━━${{NC}}"
    su - "$REAL_USER" -c "cd '{flake_path}' && git push" || echo -e "${{YELLOW}}Push failed or no remote configured${{NC}}"
else
    git add -A
    git commit -m '{escaped_msg}' || echo -e "${{YELLOW}}Nothing to commit${{NC}}"
    echo ""
    echo -e "${{BOLD}}${{CYAN}}━━━ Pushing to remote ━━━${{NC}}"
    git push || echo -e "${{YELLOW}}Push failed or no remote configured${{NC}}"
fi
echo ""

echo -e "${{BOLD}}${{GREEN}}✓ Update complete!${{NC}}"
"#,
        flake_path = flake_path,
        escaped_msg = escaped_msg,
    )
}

/// Run the update script in a background thread
fn run_update_background(script_path: &str) {
    // Run pkexec to get sudo privileges, then run the script
    // Set SHELL to a standard path to avoid pkexec rejecting nix store paths
    let mut child = match Command::new("pkexec")
        .arg("bash")
        .arg(script_path)
        .env("SHELL", "/bin/sh")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            if let Ok(mut state) = UPDATE_STATE.lock() {
                state.status_line = format!("Failed to start: {}", e);
                state.completed = true;
                state.exit_status = Some(Err(format!("Failed to start: {}", e)));
            }
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Read stdout in a separate thread
    let stdout_handle = if let Some(stdout) = stdout {
        Some(std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut state) = UPDATE_STATE.lock() {
                    state.new_output.push_str(&line);
                    state.new_output.push('\n');
                    // Update status line with the last non-empty line
                    if !line.trim().is_empty() {
                        state.status_line = line;
                    }
                }
            }
        }))
    } else {
        None
    };

    // Read stderr in a separate thread
    let stderr_handle = if let Some(stderr) = stderr {
        Some(std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut state) = UPDATE_STATE.lock() {
                    state.new_output.push_str(&line);
                    state.new_output.push('\n');
                    // Update status line with the last non-empty line
                    if !line.trim().is_empty() {
                        state.status_line = line;
                    }
                }
            }
        }))
    } else {
        None
    };

    // Wait for reader threads to finish
    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    // Wait for the process to exit
    let status = child.wait();

    // Mark completion with result
    if let Ok(mut state) = UPDATE_STATE.lock() {
        state.completed = true;
        match status {
            Ok(s) if s.success() => {
                state.status_line = "✓ Update complete!".to_string();
                state.exit_status = Some(Ok(()));
            }
            Ok(s) => {
                let msg = format!("Update failed (exit code: {:?})", s.code());
                state.status_line = msg.clone();
                state.exit_status = Some(Err(msg));
            }
            Err(e) => {
                let msg = format!("Update error: {}", e);
                state.status_line = msg.clone();
                state.exit_status = Some(Err(msg));
            }
        }
    }
}
