//! Debug log ring buffer and deferred viewport content.

use eframe::egui::{self, ScrollArea, ViewportClass};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn push_log(lines: &Arc<Mutex<VecDeque<String>>>, max_lines: usize, line: String) {
    let Ok(mut g) = lines.lock() else {
        return;
    };
    while g.len() >= max_lines {
        g.pop_front();
    }
    g.push_back(line);
}

pub fn drain_channel(
    lines: &Arc<Mutex<VecDeque<String>>>,
    rx: &std::sync::mpsc::Receiver<String>,
    max_lines: usize,
) {
    while let Ok(msg) = rx.try_recv() {
        push_log(lines, max_lines, msg);
    }
}

pub fn render_debug(ctx: &egui::Context, class: ViewportClass, title: &str, lines: &Arc<Mutex<VecDeque<String>>>) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| {
            log_scroll(ui, lines);
        });
    } else {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(title);
            log_scroll(ui, lines);
        });
    }
}

fn log_scroll(ui: &mut egui::Ui, lines: &Arc<Mutex<VecDeque<String>>>) {
    let snapshot: Vec<String> = lines
        .lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        ui.monospace(snapshot.join("\n"));
    });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
    }
}
