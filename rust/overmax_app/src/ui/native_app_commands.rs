use std::sync::atomic::Ordering;

use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::ui_command::UiCommand;

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
            UiCommand::Exit => self.exit_requested.store(true, Ordering::Relaxed),
            UiCommand::UploadCurrentPattern => {}
        }
    }

    fn open_settings(&self) {
        if self.ui_state.settings_open.load(Ordering::Relaxed) {
            if let Ok(guard) = self.ctx_holder.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.send_viewport_cmd_to(crate::system::native_helpers::vp_settings(), eframe::egui::ViewportCommand::Focus);
                }
            }
            return;
        }
        self.ui_state.settings_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            "[UI] open settings".into(),
        );
    }

    fn open_debug(&self) {
        if self.ui_state.debug_open.load(Ordering::Relaxed) {
            if let Ok(guard) = self.ctx_holder.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.send_viewport_cmd_to(crate::system::native_helpers::vp_debug(), eframe::egui::ViewportCommand::Focus);
                }
            }
            return;
        }
        self.ui_state.debug_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            "[UI] open debug".into(),
        );
    }

    fn open_sync(&self) {
        if self.ui_state.sync_open.load(Ordering::Relaxed) {
            if let Ok(guard) = self.ctx_holder.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.send_viewport_cmd_to(crate::system::native_helpers::vp_sync(), eframe::egui::ViewportCommand::Focus);
                }
            }
            return;
        }
        self.ui_state.sync_open.store(true, Ordering::Relaxed);
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            "[UI] open sync".into(),
        );
    }
}
