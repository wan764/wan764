// Hide the console window in release builds on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bot;
mod config;
mod constants;
mod dex;
mod error;
mod gui_event;
mod sniper;
mod types;
mod utils;
mod wallet;

use std::sync::Arc;
use app::SniperApp;

fn main() -> eframe::Result<()> {
    // Build a dedicated tokio runtime that runs behind the GUI
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("tokio runtime"),
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Solana Sniper Bot")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Solana Sniper Bot",
        options,
        Box::new(move |cc| {
            // Dark theme by default
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(SniperApp::new(cc, rt.clone())))
        }),
    )
}
