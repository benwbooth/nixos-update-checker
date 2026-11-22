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
        #[qproperty(bool, prune_generations)]
        #[qproperty(QString, generation_prune_mode)]
        #[qproperty(i32, generation_prune_value)]
        #[qproperty(QString, download_size)]
        #[qproperty(QString, unpacked_size)]
        #[qproperty(QString, build_info)]
        // Flatpak updates (system + user installations), JSON array
        #[qproperty(QString, flatpak_updates_json)]
        #[qproperty(i32, flatpak_count)]
        // NixOS packages + Flatpak refs
        #[qproperty(i32, total_count)]
        #[qproperty(bool, include_flatpak)]
        #[qproperty(bool, notify_updates)]
        // A notification is up and waiting for an answer
        #[qproperty(bool, notification_pending)]
        // "Reminder at 9:00 AM tomorrow" while a Later is in effect
        #[qproperty(QString, snooze_text)]
        // "NixOS 26.05 (Yarara)"
        #[qproperty(QString, system_version)]
        // A newer major NixOS release than the flake follows ("26.11"), else ""
        #[qproperty(QString, new_release)]
        // The release the flake follows ("26.05")
        #[qproperty(QString, followed_release)]
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

        /// Build the full pkexec update command string from the current
        /// updates and settings
        #[qinvokable]
        fn build_update_command(self: &UpdateChecker) -> QString;

        /// Post the "Updates Available" notification if it's due (new
        /// updates, or a Later has run out). Call after checks and once a minute.
        #[qinvokable]
        fn maybe_notify(self: Pin<&mut UpdateChecker>);

        /// The notification's answer once it has one ("install", "details",
        /// "later" already applied), else ""
        #[qinvokable]
        fn poll_notification(self: Pin<&mut UpdateChecker>) -> QString;

        /// Put off the reminder: "hour", "tonight" or "tomorrow"
        #[qinvokable]
        fn snooze(self: Pin<&mut UpdateChecker>, kind: QString);
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
use std::os::unix::fs::FileTypeExt;
use std::sync::{Arc, Mutex};

use crate::config::{Config, GenerationPruneMode, IntervalUnit};
use crate::flake_checker::{self, FlakeUpdate, CheckResult};
use crate::flatpak::{self, FlatpakUpdate};
use crate::release;

fn flatpaks_from_json(json: &str) -> Vec<FlatpakUpdate> {
    serde_json::from_str(json).unwrap_or_default()
}

fn flatpaks_to_json(updates: &[FlatpakUpdate]) -> String {
    serde_json::to_string(updates).unwrap_or_else(|_| "[]".to_string())
}

/// Identifies a set of pending updates, so a notification is posted once per set.
fn update_signature(nix: &[FlakeUpdate], flatpaks: &[FlatpakUpdate]) -> String {
    let mut parts: Vec<String> = nix.iter().map(|u| u.package_name.clone()).collect();
    parts.extend(flatpaks.iter().map(|f| format!("{}/{}@{}", f.app_id, f.branch, f.new_version)));
    parts.sort();
    parts.join("\n")
}

/// "42 NixOS packages (1.1 GiB) and 5 Flatpak apps"
fn describe_updates(nix_count: usize, download_size: &str, flatpaks: &[FlatpakUpdate]) -> String {
    let mut parts = Vec::new();
    if nix_count > 0 {
        let size = if download_size.is_empty() { String::new() } else { format!(" ({download_size})") };
        parts.push(format!(
            "{nix_count} NixOS {}{size}",
            if nix_count == 1 { "package" } else { "packages" }
        ));
    }
    if !flatpaks.is_empty() {
        let n = flatpaks.len();
        parts.push(format!("{n} Flatpak {}", if n == 1 { "update" } else { "updates" }));
    }
    parts.join(" and ")
}

/// Unix time a Later choice snoozes the reminder until.
fn snooze_target(kind: &str) -> i64 {
    use chrono::{Duration, Local, TimeZone};
    let now = Local::now();
    let at = |day: chrono::NaiveDate, hour: u32| {
        Local
            .from_local_datetime(&day.and_hms_opt(hour, 0, 0).unwrap())
            .earliest()
            .map(|t| t.timestamp())
            .unwrap_or(now.timestamp() + 3600)
    };
    match kind {
        "tonight" => {
            let nine = at(now.date_naive(), 21);
            // Already evening: a couple of hours from now.
            if nine - now.timestamp() < 3600 { now.timestamp() + 2 * 3600 } else { nine }
        }
        "tomorrow" => at(now.date_naive() + Duration::days(1), 9),
        _ => now.timestamp() + 3600,
    }
}

fn describe_snooze(until: i64) -> String {
    use chrono::{Local, TimeZone};
    let now = Local::now();
    if until <= now.timestamp() {
        return String::new();
    }
    let Some(t) = Local.timestamp_opt(until, 0).single() else {
        return String::new();
    };
    let time = t.format("%-I:%M %p");
    if t.date_naive() == now.date_naive() {
        format!("Reminder at {time}")
    } else if t.date_naive() == now.date_naive() + chrono::Duration::days(1) {
        format!("Reminder at {time} tomorrow")
    } else {
        format!("Reminder {}", t.format("%a %-I:%M %p"))
    }
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "NixOS".to_string())
}

/// A one-off notification that a new major release is out (it takes an edit
/// to flake.nix, so there's no Install button). Off the UI thread.
fn notify_new_release(release: &str, followed: &str) {
    let body = format!(
        "Your flake follows NixOS {followed}. Moving to {release} takes changing the nixpkgs input in flake.nix; read the release notes first."
    );
    let title = format!("NixOS {release} is available");
    std::thread::spawn(move || {
        let _ = std::process::Command::new("notify-send")
            .args([
                "--app-name=Software Update",
                &format!("--icon={}", notification_icon()),
                "--hint=string:desktop-entry:nixos-update-checker",
                &title,
                &body,
            ])
            .output();
    });
}

fn notification_icon() -> String {
    let in_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/icons/png/nixos-update-checker-128.png");
    if in_source.is_file() {
        in_source.to_string_lossy().into_owned()
    } else {
        "system-software-update".to_string()
    }
}

/// The answer from the notification thread (notify-send --wait).
static NOTIFY_ANSWER: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

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
    prune_generations: bool,
    generation_prune_mode: QString,
    generation_prune_value: i32,
    download_size: QString,
    unpacked_size: QString,
    build_info: QString,
    flatpak_updates_json: QString,
    flatpak_count: i32,
    total_count: i32,
    include_flatpak: bool,
    notify_updates: bool,
    notification_pending: bool,
    snooze_text: QString,
    system_version: QString,
    new_release: QString,
    followed_release: QString,
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

const UPDATE_SCRIPT: &str = "nixos-update-checker-update";

/// The script pkexec runs as root to do the update. A build from a source
/// checkout uses the checkout's scripts/ folder, wherever the binary was copied
/// to (devloop runs it from its own state folder); the package installs the
/// script next to the binary. Otherwise it's looked up on PATH, then in the
/// system profile. The path is absolute, since pkexec doesn't keep PATH.
fn update_script_path() -> String {
    let in_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(UPDATE_SCRIPT);
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(UPDATE_SCRIPT)));
    let on_path: Vec<_> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();

    std::iter::once(in_source)
        .chain(beside_exe)
        .chain(
            on_path
                .into_iter()
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(UPDATE_SCRIPT)),
        )
        .find(|script| script.is_file())
        .map(|script| script.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("/run/current-system/sw/bin/{UPDATE_SCRIPT}"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn is_socket(path: &str) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

fn systemd_user_ssh_auth_sock() -> Option<String> {
    let output = std::process::Command::new("systemctl")
        .args(["--user", "show-environment", "SSH_AUTH_SOCK"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        line.strip_prefix("SSH_AUTH_SOCK=")
            .map(str::to_string)
            .filter(|socket| !socket.is_empty() && is_socket(socket))
    })
}

fn current_ssh_auth_sock(uid: u32) -> String {
    if let Ok(socket) = std::env::var("SSH_AUTH_SOCK") {
        if is_socket(&socket) {
            return socket;
        }
    }

    if let Some(socket) = systemd_user_ssh_auth_sock() {
        return socket;
    }

    [
        format!("/run/user/{uid}/ssh-agent"),
        format!("/run/user/{uid}/keyring/ssh"),
        format!("/run/user/{uid}/gnupg/S.gpg-agent.ssh"),
    ]
    .into_iter()
    .find(|socket| is_socket(socket))
    .unwrap_or_default()
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
fn build_tooltip(updates: &[FlakeUpdate], flatpaks: &[FlatpakUpdate], last_check_timestamp: i64, download_size: &str, unpacked_size: &str) -> String {
    let last_checked = format_relative_time(last_check_timestamp);

    let mut lines = vec!["Software Update".to_string()];
    if updates.is_empty() && flatpaks.is_empty() {
        lines.push("Your system is up to date".to_string());
    }
    if !updates.is_empty() {
        lines.push(flake_checker::format_updates_tooltip(updates, download_size, unpacked_size));
    }
    if !flatpaks.is_empty() {
        let shown: Vec<String> = flatpaks.iter().take(6).map(|f| format!("  {}", f.display())).collect();
        let more = if flatpaks.len() > 6 { format!("\n  ...and {} more", flatpaks.len() - 6) } else { String::new() };
        lines.push(format!(
            "{} Flatpak {}:\n{}{}",
            flatpaks.len(),
            if flatpaks.len() == 1 { "update" } else { "updates" },
            shown.join("\n"),
            more
        ));
    }
    lines.push(format!("Last checked: {last_checked}"));
    lines.join("\n")
}

use once_cell::sync::Lazy;

/// Result from background check thread
type BgCheckResult = (
    Result<CheckResult, flake_checker::FlakeCheckError>,
    Option<Vec<FlatpakUpdate>>,
    Option<String>,
);

/// Global storage for background check result
static CHECK_RESULT: Lazy<Arc<Mutex<Option<BgCheckResult>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

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
        self.as_mut().set_tooltip_text(QString::from("Software Update\nChecking for updates..."));
        self.as_mut().check_status_changed();

        // Spawn background threads for the check: the flake and Flatpak at once.
        let flake_path_clone = flake_path.clone();
        let include_flatpak = *self.as_ref().include_flatpak();
        std::thread::spawn(move || {
            let flatpak_thread = include_flatpak.then(|| std::thread::spawn(flatpak::check_updates));
            let release_path = std::path::PathBuf::from(&flake_path_clone);
            let release_thread = std::thread::spawn(move || release::newer_release(&release_path));
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(flake_checker::check_for_updates(
                &std::path::PathBuf::from(&flake_path_clone),
            ));
            let flatpaks = flatpak_thread.and_then(|t| t.join().ok());
            let new_release = release_thread.join().ok().flatten();

            // Store result for main thread to pick up
            if let Ok(mut stored) = CHECK_RESULT.lock() {
                *stored = Some((result, flatpaks, new_release));
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

        if let Some((result, flatpaks, new_release)) = result {
            // Process the result
            let check_timestamp = chrono::Utc::now().timestamp();

            // Flatpak first: a failed flake check still reports these.
            let flatpaks = flatpaks.unwrap_or_default();
            let flatpak_json = flatpaks_to_json(&flatpaks);
            self.as_mut().set_flatpak_updates_json(QString::from(&flatpak_json));
            self.as_mut().set_flatpak_count(flatpaks.len() as i32);
            let new_release = new_release.unwrap_or_default();
            self.as_mut().set_new_release(QString::from(&new_release));
            let followed = release::followed_release(std::path::Path::new(&self.as_ref().flake_path().to_string()))
                .map(release::fmt)
                .unwrap_or_default();
            self.as_mut().set_followed_release(QString::from(&followed));
            if let Ok(mut config) = Config::load() {
                config.cached_flatpak_json = flatpak_json;
                if !new_release.is_empty() && config.release_notified != new_release && config.notify_updates {
                    config.release_notified = new_release.clone();
                    notify_new_release(&new_release, &followed);
                }
                config.new_release = new_release;
                let _ = config.save();
            }

            match result {
                Ok(check_result) => {
                    let updates = &check_result.updates;
                    let has_updates = !updates.is_empty();
                    let count = updates.len() as i32;
                    let summary = QString::from(&flake_checker::format_updates_tooltip(updates, &check_result.download_size, &check_result.unpacked_size));
                    let tooltip = QString::from(&build_tooltip(updates, &flatpaks, check_timestamp, &check_result.download_size, &check_result.unpacked_size));
                    let json = updates_to_json(updates);

                    self.as_mut().set_has_updates(has_updates || !flatpaks.is_empty());
                    self.as_mut().set_update_count(count);
                    self.as_mut().set_update_summary(summary);
                    self.as_mut().set_tooltip_text(tooltip);
                    self.as_mut().set_updates_json(QString::from(&json));
                    self.as_mut().set_download_size(QString::from(&check_result.download_size));
                    self.as_mut().set_unpacked_size(QString::from(&check_result.unpacked_size));
                    self.as_mut().set_status_message(QString::from("Check complete"));

                    // Save to config for persistence
                    if let Ok(mut config) = Config::load() {
                        config.last_check_timestamp = check_timestamp;
                        config.cached_updates_json = json;
                        config.cached_download_size = check_result.download_size.clone();
                        config.cached_unpacked_size = check_result.unpacked_size.clone();
                        let _ = config.save();
                    }
                }
                Err(e) => {
                    self.as_mut().set_status_message(QString::from(&format!("Error: {}", e)));
                    let tooltip = QString::from(&format!(
                        "Software Update\nCheck failed\nLast checked: {}",
                        format_relative_time(check_timestamp)
                    ));
                    self.as_mut().set_tooltip_text(tooltip);
                    let nix_pending = *self.as_ref().update_count() > 0;
                    self.as_mut().set_has_updates(nix_pending || !flatpaks.is_empty());

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

            let total = *self.as_ref().update_count() + *self.as_ref().flatpak_count();
            self.as_mut().set_total_count(total);
            self.as_mut().set_checking(false);
            self.as_mut().check_status_changed();
            self.as_mut().updates_changed();
            self.as_mut().maybe_notify();
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
            include_flatpak: *self.as_ref().include_flatpak(),
            notify_updates: *self.as_ref().notify_updates(),
            commit_and_push: *self.as_ref().commit_and_push(),
            run_gc_after_update: *self.as_ref().run_gc_after_update(),
            prune_generations: *self.as_ref().prune_generations(),
            generation_prune_mode: GenerationPruneMode::from_str(
                &self.as_ref().generation_prune_mode().to_string(),
            ),
            generation_prune_value: (*self.as_ref().generation_prune_value()).max(1) as u32,
            ..existing
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
                let flatpaks = if config.include_flatpak { flatpaks_from_json(&config.cached_flatpak_json) } else { Vec::new() };
                let has_updates = !updates.is_empty() || !flatpaks.is_empty();
                let count = updates.len() as i32;

                self.as_mut().set_include_flatpak(config.include_flatpak);
                self.as_mut().set_notify_updates(config.notify_updates);
                self.as_mut().set_flatpak_updates_json(QString::from(&flatpaks_to_json(&flatpaks)));
                self.as_mut().set_flatpak_count(flatpaks.len() as i32);
                self.as_mut().set_total_count(count + flatpaks.len() as i32);
                self.as_mut().set_snooze_text(QString::from(&describe_snooze(config.snooze_until)));
                self.as_mut().set_system_version(QString::from(&os_pretty_name()));
                self.as_mut().set_new_release(QString::from(&config.new_release));
                self.as_mut().set_followed_release(QString::from(
                    &release::followed_release(std::path::Path::new(&config.flake_path))
                        .map(release::fmt)
                        .unwrap_or_default(),
                ));
                self.as_mut().set_has_updates(has_updates);
                self.as_mut().set_update_count(count);
                self.as_mut().set_updates_json(QString::from(&config.cached_updates_json));

                if has_updates {
                    let summary = QString::from(&flake_checker::format_updates_tooltip(&updates, &config.cached_download_size, &config.cached_unpacked_size));
                    self.as_mut().set_update_summary(summary);
                }

                // Restore cached download/unpacked sizes
                self.as_mut().set_download_size(QString::from(&config.cached_download_size));
                self.as_mut().set_unpacked_size(QString::from(&config.cached_unpacked_size));

                // Set initial tooltip with cached status
                let tooltip = QString::from(&build_tooltip(&updates, &flatpaks, config.last_check_timestamp, &config.cached_download_size, &config.cached_unpacked_size));
                self.as_mut().set_tooltip_text(tooltip);

                self.as_mut().set_commit_and_push(config.commit_and_push);
                self.as_mut().set_run_gc_after_update(config.run_gc_after_update);
                self.as_mut().set_prune_generations(config.prune_generations);
                self.as_mut().set_generation_prune_mode(QString::from(
                    config.generation_prune_mode.as_str(),
                ));
                self.as_mut()
                    .set_generation_prune_value(config.generation_prune_value as i32);

                // Set build info - format timestamp at runtime for local timezone
                let build_epoch: i64 = std::env::var("BUILD_TIMESTAMP_EPOCH")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let build_time = if build_epoch > 0 {
                    let tz: chrono_tz::Tz = iana_time_zone::get_timezone()
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(chrono_tz::UTC);
                    chrono::DateTime::from_timestamp(build_epoch, 0)
                        .map(|dt| dt.with_timezone(&tz).format("%Y-%m-%d %H:%M:%S %Z").to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                } else {
                    "unknown".to_string()
                };
                self.as_mut().set_build_info(QString::from(&format!(
                    "Build Version: {} · {}",
                    std::env::var("BUILD_GIT_HASH").unwrap_or_else(|_| "unknown".to_string()),
                    build_time,
                )));

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
                self.as_mut().set_prune_generations(false);
                self.as_mut()
                    .set_generation_prune_mode(QString::from("keep-latest"));
                self.as_mut().set_generation_prune_value(10);
                self.as_mut().set_include_flatpak(true);
                self.as_mut().set_notify_updates(true);
                self.as_mut().set_system_version(QString::from(&os_pretty_name()));
                self.as_mut().set_tooltip_text(QString::from("Software Update\nLast checked: Never"));
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
        if *self.update_running() {
            QString::from("qrc:/icons/nix-flake-updating.svg")
        } else if *self.checking() {
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
        self.as_mut().set_download_size(QString::from(""));
        self.as_mut().set_unpacked_size(QString::from(""));
        self.as_mut().set_flatpak_updates_json(QString::from("[]"));
        self.as_mut().set_flatpak_count(0);
        self.as_mut().set_total_count(0);
        self.as_mut().set_snooze_text(QString::from(""));

        // Also clear in the config file so reloads don't repopulate
        if let Ok(mut config) = Config::load() {
            config.cached_updates_json = "[]".to_string();
            config.cached_flatpak_json = "[]".to_string();
            config.snooze_until = 0;
            config.cached_download_size = String::new();
            config.cached_unpacked_size = String::new();
            let _ = config.save();
        }

        // Update tooltip to reflect no updates
        self.as_mut().set_tooltip_text(QString::from("Software Update\nYour system is up to date"));
        self.as_mut().updates_changed();
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

                // Don't overwrite tooltip during an active update
                if !*self.as_ref().update_running() {
                    let updates = updates_from_json(&config.cached_updates_json);
                    let flatpaks = if config.include_flatpak { flatpaks_from_json(&config.cached_flatpak_json) } else { Vec::new() };
                    let tooltip = build_tooltip(&updates, &flatpaks, config.last_check_timestamp, &config.cached_download_size, &config.cached_unpacked_size);
                    self.as_mut().set_tooltip_text(QString::from(&tooltip));
                }
            }
            self.as_mut().set_snooze_text(QString::from(&describe_snooze(config.snooze_until)));
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
        // Primary: ground truth — compare booted vs current system
        // (kernel/initrd/kernel-modules), same as NixOS autoUpgrade.
        // This is authoritative right after `nixos-rebuild switch`.
        if let Some(diff) = flake_checker::booted_system_reboot_details() {
            return diff;
        }
        // Fallback (paths unreadable, e.g. non-NixOS dev): package heuristic.
        let updates_json = self.updates_json().to_string();
        let updates = updates_from_json(&updates_json);
        flake_checker::filter_reboot_packages(&updates)
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

    /// Build the full pkexec update command string from the current updates and settings
    pub fn build_update_command(&self) -> QString {
        let flake = self.flake_path().to_string();
        let nix_updates = updates_from_json(&self.updates_json().to_string());
        let msg = flake_checker::generate_commit_message(&nix_updates);
        let flatpaks = if *self.include_flatpak() {
            flatpaks_from_json(&self.flatpak_updates_json().to_string())
        } else {
            Vec::new()
        };
        // Only the parts with something to do. With nothing known (no check
        // yet), do the system, as before.
        let update_system = !nix_updates.is_empty() || flatpaks.is_empty();
        let prune_mode = GenerationPruneMode::from_str(&self.generation_prune_mode().to_string());
        let prune_value = (*self.generation_prune_value()).max(1);
        let flag = |on: bool| shell_quote(if on { "1" } else { "0" });

        let script_path = update_script_path();

        // Forward the user's agent to the root update process so root-owned flakes can push.
        let uid = unsafe { libc::getuid() };
        let ssh_auth_sock = current_ssh_auth_sock(uid);

        let cmd = format!(
            "pkexec env FLAKE_PATH={} COMMIT_MSG={} SSH_AUTH_SOCK={} ORIGINAL_UID={} UPDATE_SYSTEM={} FLATPAK_SYSTEM={} FLATPAK_USER={} COMMIT_AND_PUSH={} RUN_GC={} PRUNE_GENERATIONS={} GENERATION_PRUNE_MODE={} GENERATION_PRUNE_VALUE={} {}",
            shell_quote(&flake),
            shell_quote(&msg),
            shell_quote(&ssh_auth_sock),
            shell_quote(&uid.to_string()),
            flag(update_system),
            flag(flatpak::has_system_updates(&flatpaks)),
            flag(flatpak::has_user_updates(&flatpaks)),
            flag(*self.commit_and_push()),
            flag(*self.run_gc_after_update()),
            flag(*self.prune_generations()),
            shell_quote(prune_mode.as_str()),
            shell_quote(&prune_value.to_string()),
            shell_quote(&script_path),
        );

        QString::from(&cmd)
    }

    pub fn maybe_notify(mut self: Pin<&mut Self>) {
        if !*self.as_ref().notify_updates()
            || !*self.as_ref().has_updates()
            || *self.as_ref().update_running()
            || *self.as_ref().notification_pending()
        {
            return;
        }
        let Ok(mut config) = Config::load() else { return };
        let now = chrono::Utc::now().timestamp();
        if config.snooze_until > now {
            return;
        }
        let nix = updates_from_json(&self.as_ref().updates_json().to_string());
        let flatpaks = flatpaks_from_json(&self.as_ref().flatpak_updates_json().to_string());
        let signature = update_signature(&nix, &flatpaks);
        let snooze_ran_out = config.snooze_until > 0;
        if !snooze_ran_out && signature == config.last_notified_signature {
            return;
        }
        config.snooze_until = 0;
        config.last_notified_signature = signature;
        let _ = config.save();
        self.as_mut().set_snooze_text(QString::from(""));

        let body = format!(
            "{} {} ready to install. Install now, or later?",
            describe_updates(nix.len(), &self.as_ref().download_size().to_string(), &flatpaks),
            if nix.len() + flatpaks.len() == 1 { "is" } else { "are" },
        );
        self.as_mut().set_notification_pending(true);
        if let Ok(mut answer) = NOTIFY_ANSWER.lock() {
            *answer = None;
        }
        // notify-send blocks until the notification is answered or closed:
        // off the UI thread, with the answer picked up by poll_notification.
        std::thread::spawn(move || {
            let output = std::process::Command::new("notify-send")
                .args([
                    "--app-name=Software Update",
                    &format!("--icon={}", notification_icon()),
                    "--expire-time=0",
                    "--hint=string:desktop-entry:nixos-update-checker",
                    "--action=default=Details",
                    "--action=install=Install Now",
                    "--action=hour=In an Hour",
                    "--action=tonight=Tonight",
                    "--action=tomorrow=Tomorrow",
                    "--wait",
                    "Updates Available",
                    &body,
                ])
                .output();
            let answer = match output {
                Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                Err(e) => {
                    eprintln!("notify-send failed: {e}");
                    "error".to_string()
                }
            };
            if let Ok(mut stored) = NOTIFY_ANSWER.lock() {
                *stored = Some(answer);
            }
        });
    }

    pub fn poll_notification(mut self: Pin<&mut Self>) -> QString {
        let Some(answer) = NOTIFY_ANSWER.lock().ok().and_then(|mut a| a.take()) else {
            return QString::from("");
        };
        self.as_mut().set_notification_pending(false);
        match answer.as_str() {
            "install" => QString::from("install"),
            "default" => QString::from("details"),
            "hour" | "tonight" | "tomorrow" => {
                self.as_mut().snooze(QString::from(&answer));
                QString::from("later")
            }
            // notify-send missing or failed: nothing to put off.
            "error" => QString::from(""),
            // Closed without an answer: like macOS, ask again tomorrow.
            _ => {
                self.as_mut().snooze(QString::from("tomorrow"));
                QString::from("later")
            }
        }
    }

    pub fn snooze(mut self: Pin<&mut Self>, kind: QString) {
        let until = snooze_target(&kind.to_string());
        if let Ok(mut config) = Config::load() {
            config.snooze_until = until;
            let _ = config.save();
        }
        self.as_mut().set_snooze_text(QString::from(&describe_snooze(until)));
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
