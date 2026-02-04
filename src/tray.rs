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
        #[qproperty(bool, commit_and_push)]
        #[qproperty(bool, run_gc_after_update)]
        type UpdateChecker = super::UpdateCheckerRust;

        /// Trigger a manual update check (async - spawns background thread)
        #[qinvokable]
        fn check_now(self: Pin<&mut UpdateChecker>);

        /// Poll for check result (called by QML timer)
        #[qinvokable]
        fn poll_check_result(self: Pin<&mut UpdateChecker>);

        /// Get the flake path for the update script
        #[qinvokable]
        fn get_update_script_path(self: Pin<&mut UpdateChecker>) -> QString;

        /// Get the commit message for the update
        #[qinvokable]
        fn get_update_commit_message(self: Pin<&mut UpdateChecker>) -> QString;

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

        /// Check if system was updated externally and clear updates if so
        #[qinvokable]
        fn check_system_changed(self: Pin<&mut UpdateChecker>);

        /// Refresh the last check time display (call periodically to update relative time)
        #[qinvokable]
        fn refresh_last_check_time(self: Pin<&mut UpdateChecker>);

        /// Check if any updated packages require a reboot
        #[qinvokable]
        fn check_reboot_required(self: &UpdateChecker) -> bool;

        /// Get list of packages that require reboot (comma-separated)
        #[qinvokable]
        fn get_reboot_packages(self: &UpdateChecker) -> QString;

        /// Reboot the system
        #[qinvokable]
        fn reboot_system(self: &UpdateChecker);

        /// Clear cached updates from properties and config file (after successful update)
        #[qinvokable]
        fn clear_cached_updates(self: Pin<&mut UpdateChecker>);

        /// Find the last non-empty line from terminal text (strips ANSI codes)
        #[qinvokable]
        fn find_last_line(self: &UpdateChecker, text: QString) -> QString;

        /// Check if terminal output indicates update completion
        #[qinvokable]
        fn check_update_complete(self: &UpdateChecker, text: QString) -> bool;

        /// Get exit code from terminal output (0 = success, 1 = failure)
        #[qinvokable]
        fn get_update_exit_code(self: &UpdateChecker, text: QString) -> i32;

        /// Build the full pkexec update command string
        #[qinvokable]
        fn build_update_command(self: &UpdateChecker, flake_path: QString, commit_msg: QString, commit_and_push: bool, run_gc: bool) -> QString;
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
    commit_and_push: bool,
    run_gc_after_update: bool,
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

/// Format a timestamp as relative time ("2 hours ago", "3 days ago", etc.)
fn format_relative_time(timestamp: i64) -> String {
    if timestamp == 0 {
        return "Never".to_string();
    }

    if let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0) {
        // Use HumanTime with the DateTime directly - it will show "X ago" for past times
        let human = chrono_humanize::HumanTime::from(dt).to_string();
        // chrono_humanize returns "now" for very recent times, make it clearer
        if human == "now" {
            "just now".to_string()
        } else {
            human
        }
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

/// Cache the last known system path to detect external updates
static CACHED_SYSTEM_PATH: Lazy<Arc<Mutex<String>>> = Lazy::new(|| Arc::new(Mutex::new(String::new())));

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
        self.as_mut().set_tooltip_text(QString::from("NixOS Update Checker\nChecking for updates..."));
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

            // Update last check time (display) - use relative time
            self.as_mut().set_last_check_time(QString::from(
                &format_relative_time(check_timestamp)
            ));

            self.as_mut().set_checking(false);
            self.as_mut().check_status_changed();
            self.as_mut().updates_changed();
        }
    }

    /// Get the flake path for the update script
    pub fn get_update_script_path(self: Pin<&mut Self>) -> QString {
        // This method name is kept for QML compatibility but now returns flake path
        self.flake_path().clone()
    }

    /// Get the commit message for the update
    pub fn get_update_commit_message(self: Pin<&mut Self>) -> QString {
        let updates_json = self.updates_json().to_string();
        let updates = updates_from_json(&updates_json);
        let commit_msg = flake_checker::generate_commit_message(&updates);
        QString::from(&commit_msg)
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
            commit_and_push: *self.as_ref().commit_and_push(),
            run_gc_after_update: *self.as_ref().run_gc_after_update(),
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

                // Format last check time for display (human readable)
                if config.last_check_timestamp > 0 {
                    self.as_mut().set_last_check_time(QString::from(
                        &format_relative_time(config.last_check_timestamp),
                    ));
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

                self.as_mut().set_commit_and_push(config.commit_and_push);
                self.as_mut().set_run_gc_after_update(config.run_gc_after_update);
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

    pub fn clear_cached_updates(mut self: Pin<&mut Self>) {
        self.as_mut().set_has_updates(false);
        self.as_mut().set_update_count(0);
        self.as_mut().set_updates_json(QString::from("[]"));
        self.as_mut().set_update_summary(QString::from(""));

        // Also clear in the config file so reloads don't repopulate
        if let Ok(mut config) = Config::load() {
            config.cached_updates_json = "[]".to_string();
            let _ = config.save();
        }

        // Update tooltip to reflect no updates
        self.as_mut().set_tooltip_text(QString::from("NixOS Update Checker - No updates"));
    }

    pub fn clear_output(mut self: Pin<&mut Self>) {
        self.as_mut().set_update_output(QString::from(""));
        self.as_mut().set_update_status_line(QString::from(""));
        self.as_mut().output_changed();
    }

    /// Check if system was updated externally - if so, trigger a full recheck
    pub fn check_system_changed(mut self: Pin<&mut Self>) {
        let current_system = std::fs::read_link("/run/current-system")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut cached = match CACHED_SYSTEM_PATH.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        if cached.is_empty() {
            // First check - just cache the current value
            *cached = current_system;
            return;
        }

        if current_system != *cached {
            // System changed - update cache and trigger full recheck
            *cached = current_system;
            drop(cached); // Release lock before calling check_now

            // Only recheck if we were showing updates (optimization)
            if *self.as_ref().has_updates() {
                self.as_mut().set_status_message(QString::from("System changed, rechecking..."));
                self.as_mut().check_now();
            }
        }
    }

    /// Refresh the last check time display to show updated relative time
    pub fn refresh_last_check_time(mut self: Pin<&mut Self>) {
        if let Ok(config) = Config::load() {
            if config.last_check_timestamp > 0 {
                let relative_time = format_relative_time(config.last_check_timestamp);
                self.as_mut().set_last_check_time(QString::from(&relative_time));

                // Also update the tooltip
                let updates = updates_from_json(&config.cached_updates_json);
                let tooltip = build_tooltip(&updates, config.last_check_timestamp);
                self.as_mut().set_tooltip_text(QString::from(&tooltip));
            }
        }
    }

    /// Check if any updated packages require a reboot
    pub fn check_reboot_required(&self) -> bool {
        !self.get_reboot_packages_list().is_empty()
    }

    /// Get list of packages that require reboot (comma-separated)
    pub fn get_reboot_packages(&self) -> QString {
        QString::from(&self.get_reboot_packages_list().join(", "))
    }

    /// Internal helper to get the list of reboot-requiring packages
    fn get_reboot_packages_list(&self) -> Vec<String> {
        // Packages that typically require a reboot when updated
        const REBOOT_PACKAGES: &[&str] = &[
            "linux",
            "kernel",
            "systemd",
            "glibc",
            "dbus",
            "nvidia",
            "amdgpu",
            "mesa",
            "linux-firmware",
            "intel-microcode",
            "amd-microcode",
            "grub",
            "systemd-boot",
            "initrd",
            "firmware",
        ];

        let updates_json = self.updates_json().to_string();
        let updates = updates_from_json(&updates_json);

        let mut reboot_packages = Vec::new();
        for update in &updates {
            let name_lower = update.package_name.to_lowercase();
            for &pkg in REBOOT_PACKAGES {
                if name_lower.contains(pkg) {
                    reboot_packages.push(update.package_name.clone());
                    break;
                }
            }
        }
        reboot_packages
    }

    /// Find the last non-empty line from terminal text, stripping ANSI escape codes
    pub fn find_last_line(&self, text: QString) -> QString {
        let text_str = text.to_string();
        let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        for line in text_str.lines().rev() {
            let cleaned = ansi_re.replace_all(line.trim(), "").to_string();
            if !cleaned.is_empty() {
                return QString::from(&cleaned);
            }
        }
        QString::from("")
    }

    /// Check if terminal output indicates update completion
    pub fn check_update_complete(&self, text: QString) -> bool {
        let text_str = text.to_string();
        let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        let clean = ansi_re.replace_all(&text_str, "").to_string();
        clean.contains("Update complete!") || clean.contains("Update failed")
    }

    /// Get exit code from terminal output (0 = success, 1 = failure)
    pub fn get_update_exit_code(&self, text: QString) -> i32 {
        let text_str = text.to_string();
        let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        let clean = ansi_re.replace_all(&text_str, "").to_string();
        if clean.contains("Update failed") || clean.contains("\u{2717}") {
            1
        } else {
            0
        }
    }

    /// Build the full pkexec update command string
    pub fn build_update_command(&self, flake_path: QString, commit_msg: QString, commit_and_push: bool, run_gc: bool) -> QString {
        let flake = flake_path.to_string();
        let msg = commit_msg.to_string();

        // Find the script - check dev location first, then installed location
        let exe_path = std::env::current_exe().unwrap_or_default();
        let dev_script = exe_path
            .parent()
            .map(|p| p.join("../../scripts/nixos-update-checker-update"))
            .and_then(|p| std::fs::canonicalize(p).ok());

        let script_path = match dev_script {
            Some(p) => p.to_string_lossy().to_string(),
            None => "/run/current-system/sw/bin/nixos-update-checker-update".to_string(),
        };

        // Get SSH_AUTH_SOCK for git push passthrough
        let ssh_auth_sock = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();

        let cmd = format!(
            "pkexec env FLAKE_PATH='{}' COMMIT_MSG='{}' SSH_AUTH_SOCK='{}' COMMIT_AND_PUSH='{}' RUN_GC='{}' '{}'",
            flake,
            msg,
            ssh_auth_sock,
            if commit_and_push { "1" } else { "0" },
            if run_gc { "1" } else { "0" },
            script_path,
        );

        QString::from(&cmd)
    }

    /// Reboot the system using loginctl (works without root via polkit)
    pub fn reboot_system(&self) {
        eprintln!("Initiating system reboot...");
        // loginctl reboot works for regular users via polkit/D-Bus
        let result = std::process::Command::new("loginctl")
            .arg("reboot")
            .spawn();

        if result.is_err() {
            // Fallback to systemctl
            let _ = std::process::Command::new("systemctl")
                .arg("reboot")
                .spawn();
        }
    }
}

