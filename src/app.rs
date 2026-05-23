use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{Color32, RichText, ScrollArea, Ui};

use crate::config::Config;
use crate::gui_event::{GuiEvent, LogEntry, LogLevel};

// ── Tab ────────────────────────────────────────────────────────

#[derive(PartialEq)]
enum Tab {
    Feed,
    Positions,
    Stats,
}

// ── Position row ───────────────────────────────────────────────

struct PositionRow {
    pool:      String,
    dex:       String,
    sol_in:    f64,
    pnl:       f64,
    status:    String,
    open_time: String,
}

// ── Stats ──────────────────────────────────────────────────────

#[derive(Default)]
struct AppStats {
    pools_detected: u64,
    trades:         u64,
    wins:           u64,
    total_pnl:      f64,
}

// ── Main App ───────────────────────────────────────────────────

pub struct SniperApp {
    // ─ Config fields ──────────────────────────────────────────
    rpc_endpoint:      String,
    ws_endpoint:       String,
    private_key:       String,
    show_key:          bool,
    buy_sol:           f64,
    slippage_bps:      u64,
    compute_price:     u64,
    use_jito:          bool,
    jito_tip:          u64,

    en_raydium_amm:    bool,
    en_raydium_cpmm:   bool,
    en_orca:           bool,
    en_meteora_dlmm:   bool,
    en_meteora_dammv2: bool,

    min_liq_sol:       f64,
    reject_mint_auth:  bool,
    reject_freeze_auth: bool,

    auto_sell:         bool,
    take_profit_x:     f64,
    stop_loss_x:       f64,
    max_hold_secs:     u64,

    // ─ Spam tx ────────────────────────────────────────────────
    spam_enabled:      bool,
    spam_count:        u32,
    min_out_amount:    f64,
    min_out_decimals:  u8,
    spam_delay_ms:     u64,
    stop_on_success:   bool,

    // ─ Runtime state ──────────────────────────────────────────
    bot_running:  bool,
    active_tab:   Tab,
    log:          VecDeque<LogEntry>,
    positions:    Vec<PositionRow>,
    stats:        AppStats,

    // ─ Channels / runtime ─────────────────────────────────────
    event_rx:   mpsc::Receiver<GuiEvent>,
    event_tx:   mpsc::Sender<GuiEvent>,
    rt:         std::sync::Arc<tokio::runtime::Runtime>,
    bot_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SniperApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        rt: std::sync::Arc<tokio::runtime::Runtime>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();

        dotenv::dotenv().ok();
        let rpc = std::env::var("RPC_ENDPOINT").unwrap_or_default();
        let ws  = std::env::var("RPC_WS_ENDPOINT").unwrap_or_default();
        let key = std::env::var("PRIVATE_KEY").unwrap_or_default();

        Self {
            rpc_endpoint:      rpc,
            ws_endpoint:       ws,
            private_key:       key,
            show_key:          false,
            buy_sol:           0.1,
            slippage_bps:      1500,
            compute_price:     100_000,
            use_jito:          true,
            jito_tip:          100_000,
            en_raydium_amm:    true,
            en_raydium_cpmm:   true,
            en_orca:           true,
            en_meteora_dlmm:   true,
            en_meteora_dammv2: true,
            min_liq_sol:       1.0,
            reject_mint_auth:  true,
            reject_freeze_auth: true,
            auto_sell:         true,
            take_profit_x:     2.0,
            stop_loss_x:       0.5,
            max_hold_secs:     300,
            spam_enabled:      false,
            spam_count:        10,
            min_out_amount:    0.0,
            min_out_decimals:  6,
            spam_delay_ms:     500,
            stop_on_success:   true,
            bot_running:       false,
            active_tab:        Tab::Feed,
            log:               VecDeque::with_capacity(500),
            positions:         Vec::new(),
            stats:             AppStats::default(),
            event_rx:          rx,
            event_tx:          tx,
            rt,
            bot_handle:        None,
        }
    }

    // ── Bot control ────────────────────────────────────────────

    fn build_config(&self) -> Config {
        Config {
            rpc_url:                    self.rpc_endpoint.clone(),
            ws_url:                     self.ws_endpoint.clone(),
            private_key:                self.private_key.clone(),
            use_jito:                   self.use_jito,
            jito_tip_lamports:          self.jito_tip,
            jito_block_engine_url:      "https://mainnet.block-engine.jito.wtf".to_string(),
            buy_amount_lamports:        (self.buy_sol * 1_000_000_000.0) as u64,
            max_slippage_bps:           self.slippage_bps,
            compute_unit_price:         self.compute_price,
            compute_unit_limit:         300_000,
            enable_raydium_amm:         self.en_raydium_amm,
            enable_raydium_cpmm:        self.en_raydium_cpmm,
            enable_orca:                self.en_orca,
            enable_meteora_dlmm:        self.en_meteora_dlmm,
            enable_meteora_dammv2:      self.en_meteora_dammv2,
            min_pool_liquidity_lamports: (self.min_liq_sol * 1_000_000_000.0) as u64,
            quote_mints: vec![
                "So11111111111111111111111111111111111111112"
                    .parse()
                    .unwrap(),
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                    .parse()
                    .unwrap(),
            ],
            reject_mint_authority:  self.reject_mint_auth,
            reject_freeze_authority: self.reject_freeze_auth,
            auto_sell:              self.auto_sell,
            take_profit_x:          self.take_profit_x,
            stop_loss_x:            self.stop_loss_x,
            max_hold_secs:          self.max_hold_secs,
            position_check_ms:      5_000,
            spam_enabled:           self.spam_enabled,
            spam_count:             self.spam_count,
            min_out_amount:         self.min_out_amount,
            min_out_decimals:       self.min_out_decimals,
            spam_delay_ms:          self.spam_delay_ms,
            stop_on_success:        self.stop_on_success,
        }
    }

    fn start_bot(&mut self) {
        let config = self.build_config();
        let tx     = self.event_tx.clone();
        let handle = self.rt.spawn(async move {
            crate::bot::run_bot(config, tx).await;
        });
        self.bot_handle = Some(handle);
        self.bot_running = true;
        self.push_log(LogLevel::Success, "▶ Bot started".to_string());
    }

    fn stop_bot(&mut self) {
        if let Some(h) = self.bot_handle.take() {
            h.abort();
        }
        crate::gui_event::clear_sender();
        self.bot_running = false;
        self.push_log(LogLevel::Warning, "⏹ Bot stopped".to_string());
    }

    fn push_log(&mut self, level: LogLevel, message: String) {
        let s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ts = format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60);
        if self.log.len() >= 500 {
            self.log.pop_front();
        }
        self.log.push_back(LogEntry { timestamp: ts, level, message });
    }

    fn poll_events(&mut self) {
        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                GuiEvent::Log(entry) => {
                    if self.log.len() >= 500 {
                        self.log.pop_front();
                    }
                    self.log.push_back(entry);
                }
                GuiEvent::PositionOpened { pool, dex, sol_in } => {
                    let s = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let ts = format!("{:02}:{:02}:{:02}", (s/3600)%24, (s/60)%60, s%60);
                    self.positions.push(PositionRow {
                        pool:      pool.clone(),
                        dex,
                        sol_in,
                        pnl:       0.0,
                        status:    "Open".to_string(),
                        open_time: ts,
                    });
                    self.stats.trades += 1;
                }
                GuiEvent::PositionClosed { pool, pnl, reason } => {
                    if let Some(p) = self.positions.iter_mut().find(|p| p.pool == pool) {
                        p.pnl    = pnl;
                        p.status = format!("Closed ({})", reason);
                    }
                    self.stats.total_pnl += pnl;
                    if pnl > 0.0 { self.stats.wins += 1; }
                }
                GuiEvent::BotStopped => {
                    self.bot_running = false;
                }
            }
        }
    }

    // ── Panel renderers ────────────────────────────────────────

    fn render_settings(&mut self, ui: &mut Ui) -> bool {
        let mut start_clicked = false;
        let mut stop_clicked  = false;

        ScrollArea::vertical()
            .id_source("settings_scroll")
            .show(ui, |ui| {
                ui.add_space(4.0);

                // ─ Network ──────────────────────────────────
                egui::CollapsingHeader::new("🔗  Network")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("net_grid")
                            .num_columns(2)
                            .spacing([4.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("RPC");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.rpc_endpoint)
                                        .hint_text("https://mainnet.helius…")
                                        .desired_width(200.0),
                                );
                                ui.end_row();
                                ui.label("WS");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.ws_endpoint)
                                        .hint_text("wss://mainnet.helius…")
                                        .desired_width(200.0),
                                );
                                ui.end_row();
                            });
                    });

                ui.add_space(4.0);

                // ─ Wallet ───────────────────────────────────
                egui::CollapsingHeader::new("🔑  Wallet")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.private_key)
                                    .hint_text("Base-58 private key")
                                    .password(!self.show_key)
                                    .desired_width(190.0),
                            );
                            let eye = if self.show_key { "🙈" } else { "👁" };
                            if ui.small_button(eye).clicked() {
                                self.show_key = !self.show_key;
                            }
                        });
                    });

                ui.add_space(4.0);

                // ─ Trading ──────────────────────────────────
                egui::CollapsingHeader::new("💰  Trading")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("trade_grid")
                            .num_columns(2)
                            .spacing([4.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Buy Amount");
                                ui.add(
                                    egui::DragValue::new(&mut self.buy_sol)
                                        .speed(0.01)
                                        .range(0.001..=100.0)
                                        .fixed_decimals(3)
                                        .suffix(" SOL"),
                                );
                                ui.end_row();
                                ui.label("Slippage");
                                ui.add(
                                    egui::DragValue::new(&mut self.slippage_bps)
                                        .speed(50)
                                        .range(10u64..=5_000u64)
                                        .suffix(" bps"),
                                );
                                ui.end_row();
                                ui.label("Priority");
                                ui.add(
                                    egui::DragValue::new(&mut self.compute_price)
                                        .speed(1_000)
                                        .range(1_000u64..=10_000_000u64)
                                        .suffix(" µlamp"),
                                );
                                ui.end_row();
                                ui.label("");
                                ui.checkbox(&mut self.use_jito, "Jito Bundles");
                                ui.end_row();
                                if self.use_jito {
                                    ui.label("Jito Tip");
                                    ui.add(
                                        egui::DragValue::new(&mut self.jito_tip)
                                            .speed(1_000)
                                            .range(1_000u64..=2_000_000u64)
                                            .suffix(" lamp"),
                                    );
                                    ui.end_row();
                                }
                            });
                    });

                ui.add_space(4.0);

                // ─ DEXes ────────────────────────────────────
                egui::CollapsingHeader::new("🏪  DEXes")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.checkbox(&mut self.en_raydium_amm,    "Raydium AMM V4");
                        ui.checkbox(&mut self.en_raydium_cpmm,   "Raydium CPMM");
                        ui.checkbox(&mut self.en_orca,           "Orca Whirlpools");
                        ui.checkbox(&mut self.en_meteora_dlmm,   "Meteora DLMM");
                        ui.checkbox(&mut self.en_meteora_dammv2, "Meteora DAMMv2");
                    });

                ui.add_space(4.0);

                // ─ Filters ──────────────────────────────────
                egui::CollapsingHeader::new("🔍  Filters")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Min Liquidity");
                            ui.add(
                                egui::DragValue::new(&mut self.min_liq_sol)
                                    .speed(0.5)
                                    .range(0.0..=1_000.0)
                                    .suffix(" SOL"),
                            );
                        });
                        ui.checkbox(&mut self.reject_mint_auth,  "Reject Mint Authority");
                        ui.checkbox(&mut self.reject_freeze_auth, "Reject Freeze Authority");
                    });

                ui.add_space(4.0);

                // ─ Auto-Sell ────────────────────────────────
                egui::CollapsingHeader::new("📈  Auto-Sell")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.checkbox(&mut self.auto_sell, "Enable Auto-Sell");
                        if self.auto_sell {
                            egui::Grid::new("autosell_grid")
                                .num_columns(2)
                                .spacing([4.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Take Profit");
                                    ui.add(
                                        egui::DragValue::new(&mut self.take_profit_x)
                                            .speed(0.1)
                                            .range(1.05..=100.0)
                                            .fixed_decimals(2)
                                            .suffix(" ×"),
                                    );
                                    ui.end_row();
                                    ui.label("Stop Loss");
                                    ui.add(
                                        egui::DragValue::new(&mut self.stop_loss_x)
                                            .speed(0.01)
                                            .range(0.01..=0.99)
                                            .fixed_decimals(2)
                                            .suffix(" ×"),
                                    );
                                    ui.end_row();
                                    ui.label("Max Hold");
                                    ui.add(
                                        egui::DragValue::new(&mut self.max_hold_secs)
                                            .speed(10)
                                            .range(10u64..=7_200u64)
                                            .suffix(" s"),
                                    );
                                    ui.end_row();
                                });
                        }
                    });

                ui.add_space(4.0);

                // ─ Spam Tx ──────────────────────────────────
                egui::CollapsingHeader::new("🔁  Spam Tx")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.checkbox(&mut self.spam_enabled, "Enable Spam Mode");
                        if self.spam_enabled {
                            egui::Grid::new("spam_grid")
                                .num_columns(2)
                                .spacing([4.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Max Attempts");
                                    ui.add(
                                        egui::DragValue::new(&mut self.spam_count)
                                            .speed(1)
                                            .range(1u32..=500u32),
                                    );
                                    ui.end_row();

                                    ui.label("Min Out Amount");
                                    ui.add(
                                        egui::DragValue::new(&mut self.min_out_amount)
                                            .speed(1.0)
                                            .range(0.0..=1_000_000.0)
                                            .fixed_decimals(2),
                                    );
                                    ui.end_row();

                                    ui.label("Out Decimals");
                                    ui.add(
                                        egui::DragValue::new(&mut self.min_out_decimals)
                                            .speed(1)
                                            .range(0u8..=18u8),
                                    );
                                    ui.end_row();

                                    ui.label("Delay");
                                    ui.add(
                                        egui::DragValue::new(&mut self.spam_delay_ms)
                                            .speed(50)
                                            .range(0u64..=5_000u64)
                                            .suffix(" ms"),
                                    );
                                    ui.end_row();

                                    ui.label("");
                                    ui.checkbox(&mut self.stop_on_success, "Stop on Success");
                                    ui.end_row();
                                });

                            ui.label(
                                egui::RichText::new(
                                    "⚠ Spam fires real on-chain txs.\nSet Min Out to 0 to skip output check.",
                                )
                                .color(egui::Color32::YELLOW)
                                .small(),
                            );
                        }
                    });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // ─ Start / Stop ─────────────────────────────
                let ready = !self.rpc_endpoint.is_empty()
                    && !self.ws_endpoint.is_empty()
                    && !self.private_key.is_empty();

                if self.bot_running {
                    let btn = egui::Button::new(
                        RichText::new("⏹  STOP BOT")
                            .color(Color32::WHITE)
                            .strong()
                            .size(16.0),
                    )
                    .fill(Color32::from_rgb(180, 40, 40))
                    .min_size([265.0, 42.0].into());
                    if ui.add(btn).clicked() {
                        stop_clicked = true;
                    }
                } else {
                    ui.add_enabled_ui(ready, |ui| {
                        let btn = egui::Button::new(
                            RichText::new("▶  START BOT")
                                .color(Color32::WHITE)
                                .strong()
                                .size(16.0),
                        )
                        .fill(Color32::from_rgb(30, 140, 30))
                        .min_size([265.0, 42.0].into());
                        if ui.add(btn).clicked() {
                            start_clicked = true;
                        }
                    });
                    if !ready {
                        ui.label(
                            RichText::new("⚠ Fill RPC + WS + Key first")
                                .color(Color32::YELLOW)
                                .small(),
                        );
                    }
                }

                ui.add_space(6.0);
                let (status_txt, status_col) = if self.bot_running {
                    ("● RUNNING", Color32::from_rgb(80, 220, 80))
                } else {
                    ("○ IDLE", Color32::DARK_GRAY)
                };
                ui.label(RichText::new(status_txt).color(status_col).small().strong());
            });

        if stop_clicked  { self.stop_bot(); }
        if start_clicked { self.start_bot(); }
        start_clicked || stop_clicked
    }

    fn render_feed(&self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_source("feed_scroll")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in &self.log {
                    let (label_col, msg_col) = match entry.level {
                        LogLevel::Success => (
                            Color32::from_rgb(80, 200, 80),
                            Color32::from_rgb(180, 255, 180),
                        ),
                        LogLevel::Warning => (
                            Color32::from_rgb(220, 180, 50),
                            Color32::from_rgb(255, 220, 100),
                        ),
                        LogLevel::Error => (
                            Color32::from_rgb(230, 60, 60),
                            Color32::from_rgb(255, 150, 150),
                        ),
                        LogLevel::Info => (
                            Color32::from_rgb(120, 170, 220),
                            Color32::from_rgb(200, 220, 245),
                        ),
                    };
                    let level_str = match entry.level {
                        LogLevel::Success => "DONE",
                        LogLevel::Warning => "WARN",
                        LogLevel::Error   => "ERR ",
                        LogLevel::Info    => "INFO",
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&entry.timestamp)
                                .color(Color32::DARK_GRAY)
                                .monospace()
                                .small(),
                        );
                        ui.label(
                            RichText::new(level_str)
                                .color(label_col)
                                .monospace()
                                .small()
                                .strong(),
                        );
                        ui.label(
                            RichText::new(&entry.message)
                                .color(msg_col)
                                .monospace()
                                .small(),
                        );
                    });
                }
                if self.log.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Activity feed — start the bot to see events here")
                                .color(Color32::DARK_GRAY),
                        );
                    });
                }
            });
    }

    fn render_positions(&self, ui: &mut Ui) {
        if self.positions.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("No positions yet")
                        .color(Color32::DARK_GRAY),
                );
            });
            return;
        }

        ScrollArea::both()
            .id_source("pos_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Grid::new("pos_grid")
                    .num_columns(6)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        // Header row
                        for h in &["Pool", "DEX", "In (SOL)", "P&L (SOL)", "Status", "Time"] {
                            ui.label(RichText::new(*h).strong());
                        }
                        ui.end_row();

                        for pos in &self.positions {
                            let len = pos.pool.len();
                            let short = if len >= 8 {
                                format!("{}…{}", &pos.pool[..4], &pos.pool[len - 4..])
                            } else {
                                pos.pool.clone()
                            };
                            ui.monospace(&short);
                            ui.label(&pos.dex);
                            ui.label(format!("{:.4}", pos.sol_in));

                            let (pnl_txt, pnl_col) = if pos.pnl > 0.0 {
                                (
                                    format!("+{:.4}", pos.pnl),
                                    Color32::from_rgb(80, 200, 80),
                                )
                            } else if pos.pnl < 0.0 {
                                (
                                    format!("{:.4}", pos.pnl),
                                    Color32::from_rgb(230, 60, 60),
                                )
                            } else {
                                ("0.0000".to_string(), Color32::GRAY)
                            };
                            ui.label(RichText::new(pnl_txt).color(pnl_col));

                            let st_col = if pos.status == "Open" {
                                Color32::from_rgb(80, 200, 80)
                            } else {
                                Color32::GRAY
                            };
                            ui.label(RichText::new(&pos.status).color(st_col));
                            ui.label(&pos.open_time);
                            ui.end_row();
                        }
                    });
            });
    }

    fn render_stats(&self, ui: &mut Ui) {
        ui.add_space(20.0);
        let pnl_col = if self.stats.total_pnl >= 0.0 {
            Color32::from_rgb(80, 200, 80)
        } else {
            Color32::from_rgb(230, 60, 60)
        };

        egui::Grid::new("stats_grid")
            .num_columns(2)
            .spacing([60.0, 12.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Pools Detected").strong());
                ui.label(
                    RichText::new(self.stats.pools_detected.to_string()).monospace(),
                );
                ui.end_row();

                ui.label(RichText::new("Total Trades").strong());
                ui.label(
                    RichText::new(self.stats.trades.to_string()).monospace(),
                );
                ui.end_row();

                ui.label(RichText::new("Wins").strong());
                ui.label(
                    RichText::new(self.stats.wins.to_string())
                        .monospace()
                        .color(Color32::from_rgb(80, 200, 80)),
                );
                ui.end_row();

                ui.label(RichText::new("Win Rate").strong());
                let wr = if self.stats.trades > 0 {
                    format!(
                        "{:.1} %",
                        self.stats.wins as f64 / self.stats.trades as f64 * 100.0
                    )
                } else {
                    "—".to_string()
                };
                ui.label(RichText::new(wr).monospace());
                ui.end_row();

                ui.label(RichText::new("Total P&L").strong());
                let sign = if self.stats.total_pnl >= 0.0 { "+" } else { "" };
                ui.label(
                    RichText::new(format!("{sign}{:.6} SOL", self.stats.total_pnl))
                        .monospace()
                        .strong()
                        .color(pnl_col),
                );
                ui.end_row();
            });
    }
}

// ── eframe::App ────────────────────────────────────────────────

impl eframe::App for SniperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pull any events from bot tasks
        self.poll_events();

        // Repaint every 250 ms while running so the feed stays live
        if self.bot_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        // ── Left settings panel ─────────────────────────────
        egui::SidePanel::left("settings_panel")
            .exact_width(295.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.heading(
                    RichText::new("🎯  Solana Sniper Bot")
                        .strong()
                        .size(16.0),
                );
                ui.separator();
                self.render_settings(ui);
            });

        // ── Main content panel ──────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.active_tab,
                    Tab::Feed,
                    RichText::new("📋  Activity").strong(),
                );
                ui.selectable_value(
                    &mut self.active_tab,
                    Tab::Positions,
                    RichText::new(format!("📊  Positions ({})", self.positions.len())).strong(),
                );
                ui.selectable_value(
                    &mut self.active_tab,
                    Tab::Stats,
                    RichText::new("📈  Stats").strong(),
                );
            });
            ui.separator();

            match self.active_tab {
                Tab::Feed      => self.render_feed(ui),
                Tab::Positions => self.render_positions(ui),
                Tab::Stats     => self.render_stats(ui),
            }
        });
    }
}
