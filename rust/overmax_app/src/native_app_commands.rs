use std::sync::atomic::Ordering;

use crate::debug_ui;
use crate::native_app::NativeApp;
use crate::ui_command::UiCommand;

impl NativeApp {
    pub(crate) fn drain_ui_commands(&mut self) -> bool {
        let mut handled = false;
        while let Ok(command) = self.ui_cmd_rx.try_recv() {
            self.handle_ui_command(command);
            handled = true;
        }
        handled
    }

    pub(crate) fn handle_ui_command(&self, command: UiCommand) {
        match command {
            UiCommand::OpenSettings => self.open_settings(),
            UiCommand::OpenDebug => self.open_debug(),
            UiCommand::OpenSync => self.open_sync(),
            UiCommand::ToggleOverlay => self.toggle_overlay(),
            UiCommand::Exit => self.exit_requested.store(true, Ordering::Relaxed),
        }
    }

    fn open_settings(&self) {
        self.settings_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.log_lines,
            self.max_log_lines(),
            "[UI] open settings".into(),
        );
    }

    fn open_debug(&self) {
        self.debug_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.log_lines,
            self.max_log_lines(),
            "[UI] open debug".into(),
        );
    }

    fn open_sync(&self) {
        self.sync_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.log_lines,
            self.max_log_lines(),
            "[UI] open sync".into(),
        );
    }

    fn toggle_overlay(&self) {
        let current = self.overlay_visible.load(Ordering::Relaxed);
        self.overlay_visible.store(!current, Ordering::Relaxed);
        debug_ui::push_log(
            &self.log_lines,
            self.max_log_lines(),
            "[UI] toggle overlay".into(),
        );
    }
}
