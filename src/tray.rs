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
        #[qproperty(QString, terminal)]
        #[qproperty(bool, checking)]
        #[qproperty(QString, last_check_time)]
        #[qproperty(QString, status_message)]
        // Store updates as a JSON string for simplicity
        #[qproperty(QString, updates_json)]
        type UpdateChecker = super::UpdateCheckerRust;

        /// Trigger a manual update check
        #[qinvokable]
        fn check_now(self: Pin<&mut UpdateChecker>);

        /// Run the actual update (opens terminal)
        #[qinvokable]
        fn run_update(self: Pin<&mut UpdateChecker>);

        /// Save configuration
        #[qinvokable]
        fn save_config(self: Pin<&mut UpdateChecker>, flake_path: QString, interval: i32, terminal: QString);

        /// Load configuration
        #[qinvokable]
        fn load_config(self: Pin<&mut UpdateChecker>);

        /// Get the icon path based on update state
        #[qinvokable]
        fn get_icon_path(self: &UpdateChecker) -> QString;

        /// Quit the application
        #[qinvokable]
        fn quit_app(self: &UpdateChecker);
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
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

use crate::config::Config;
use crate::flake_checker::{self, FlakeUpdate};
use crate::update_runner;

/// Rust implementation of the UpdateChecker QObject
#[derive(Default)]
pub struct UpdateCheckerRust {
    has_updates: bool,
    update_count: i32,
    update_summary: QString,
    tooltip_text: QString,
    flake_path: QString,
    check_interval: i32,
    terminal: QString,
    checking: bool,
    last_check_time: QString,
    status_message: QString,
    updates_json: QString,
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

    pub fn run_update(self: Pin<&mut Self>) {
        let flake_path = self.flake_path().to_string();
        let terminal = self.terminal().to_string();
        let updates_json = self.updates_json().to_string();

        if flake_path.is_empty() {
            return;
        }

        // Parse updates from JSON
        let updates = updates_from_json(&updates_json);

        std::thread::spawn(move || {
            let _ = update_runner::run_update(&flake_path, &terminal, &updates);
        });
    }

    pub fn save_config(mut self: Pin<&mut Self>, flake_path: QString, interval: i32, terminal: QString) {
        let config = Config {
            flake_path: flake_path.to_string(),
            check_interval_minutes: interval as u32,
            terminal: terminal.to_string(),
        };

        if let Err(e) = config.save() {
            self.as_mut().set_status_message(QString::from(&format!("Failed to save config: {}", e)));
            return;
        }

        self.as_mut().set_flake_path(flake_path);
        self.as_mut().set_check_interval(interval);
        self.as_mut().set_terminal(terminal);
        self.as_mut().set_status_message(QString::from("Configuration saved"));
        self.as_mut().config_saved();
    }

    pub fn load_config(mut self: Pin<&mut Self>) {
        match Config::load() {
            Ok(config) => {
                self.as_mut().set_flake_path(QString::from(&config.flake_path));
                self.as_mut().set_check_interval(config.check_interval_minutes as i32);
                self.as_mut().set_terminal(QString::from(&config.terminal));
                self.as_mut().set_status_message(QString::from("Configuration loaded"));
                self.as_mut().config_loaded();
            }
            Err(e) => {
                self.as_mut().set_status_message(QString::from(&format!("Failed to load config: {}", e)));
                // Set defaults
                self.as_mut().set_check_interval(60);
                self.as_mut().set_terminal(QString::from("ghostty"));
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
}
