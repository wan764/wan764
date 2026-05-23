//! GUI event channel — lets any part of the bot push events to the UI.
//!
//! Usage from async bot code:
//!   gui::log(LogLevel::Success, "BUY sent: …");
//!   gui::position_opened("Pool123", "Raydium CPMM", 0.1);

use std::sync::{mpsc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Event types ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum GuiEvent {
    Log(LogEntry),
    PositionOpened { pool: String, dex: String, sol_in: f64 },
    PositionClosed { pool: String, pnl: f64, reason: String },
    BotStopped,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

// ── Global sender (set when bot starts, cleared on stop) ──────

static SENDER: Mutex<Option<mpsc::Sender<GuiEvent>>> = Mutex::new(None);

pub fn set_sender(tx: mpsc::Sender<GuiEvent>) {
    if let Ok(mut g) = SENDER.lock() {
        *g = Some(tx);
    }
}

pub fn clear_sender() {
    if let Ok(mut g) = SENDER.lock() {
        *g = None;
    }
}

fn send(ev: GuiEvent) {
    if let Ok(g) = SENDER.lock() {
        if let Some(tx) = g.as_ref() {
            let _ = tx.send(ev);
        }
    }
}

// ── Helper fns used throughout the bot ────────────────────────

fn now_hms() -> String {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

pub fn log(level: LogLevel, message: impl Into<String>) {
    send(GuiEvent::Log(LogEntry {
        timestamp: now_hms(),
        level,
        message: message.into(),
    }));
}

pub fn info(msg: impl Into<String>)    { log(LogLevel::Info,    msg) }
pub fn success(msg: impl Into<String>) { log(LogLevel::Success, msg) }
pub fn warn(msg: impl Into<String>)    { log(LogLevel::Warning, msg) }
pub fn error(msg: impl Into<String>)   { log(LogLevel::Error,   msg) }

pub fn position_opened(pool: &str, dex: &str, sol_in: f64) {
    send(GuiEvent::PositionOpened {
        pool: pool.to_string(),
        dex:  dex.to_string(),
        sol_in,
    });
}

pub fn position_closed(pool: &str, pnl: f64, reason: &str) {
    send(GuiEvent::PositionClosed {
        pool:   pool.to_string(),
        pnl,
        reason: reason.to_string(),
    });
}
