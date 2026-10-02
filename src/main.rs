pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use iced::{Task, Subscription};
use tokio::runtime::Builder;
use tokio::sync::mpsc;

pub mod bridge;
pub mod btc_script_type;
pub mod channel;
pub mod controller;
pub mod decrypt;
pub mod encrypt;
pub mod gate;
pub mod icon;
pub mod secure;
pub mod startup;
pub mod startup_trace;
pub mod ui;
pub mod wallet;
pub mod utils;
pub mod ws;

use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ws::run_websocket;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Disable core dumps + (Linux) ptrace/proc-mem access before anything runs,
    // so a crash or same-user process can't extract mlocked secrets. Release-only.
    secure::harden_process();
    startup_trace::stamp("main");

    startup::init_globals();

    // iced indexes every system font before its first frame, twice (text, then
    // the SVG renderer) — seconds on the first launch after a boot. Turn both
    // scans off and build the font system now (utils/fonts.rs); the fonts are
    // read behind the window, once its first frame is out. This sets an
    // environment variable, so it has to come before the runtime: the process
    // must still be one thread.
    utils::fonts::index_before_window();
    startup_trace::stamp("font system ready");
    startup_trace::watch_libraries();

    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;

    let handle = runtime.handle().clone();

    // ONE socket to the proxy (workspace/TRANSPORT-SPEC.md). Both sockets open
    // unconditionally at startup before; the one does too — nothing here is
    // gated on a wallet existing.
    let (rates_cmd_tx, rates_cmd_rx) = mpsc::channel::<crate::ws::RatesCommand>(32);
    let _ = crate::ws::RATES_COMMANDS_TX.set(rates_cmd_tx);
    let (commands_tx, commands_rx) = mpsc::channel::<crate::channel::WSCommand>(100);
    let (outgoing_tx, outgoing_rx) = mpsc::channel(100);
    let (ws_shutdown_tx, ws_shutdown_rx) = mpsc::channel::<()>(1);

    let tx_for_wallet = commands_tx.clone();
    let _ = crate::ws::CRYPTO_COMMANDS_TX.set(commands_tx);
    let _ = crate::ws::CRYPTO_OUTGOING_TX.set(outgoing_tx);
    let _ = crate::ws::WS_SHUTDOWN_TX.set(ws_shutdown_tx);

    let mut join_handles: Vec<tokio::task::JoinHandle<()>> = vec![];

    let ws_handle = handle.spawn(async move {
        let _ = run_websocket(commands_rx, outgoing_rx, rates_cmd_rx, ws_shutdown_rx).await;
    });
    join_handles.push(ws_handle);

    let wallet_handle = handle.spawn_blocking(move || {
        wallet::load_wallets(tx_for_wallet);
    });
    join_handles.push(wallet_handle);

    let _guard = runtime.enter();

    startup_trace::stamp("runtime up, entering iced");
    let ran = iced::application(
        || {
            startup_trace::stamp("event loop up, app booted");
            (AppState::default(), Task::none())
        },
        update,
        view,
    )
    // Matched by the family names inside the files — "Inter 18pt" and
    // "JetBrains Mono" (utils/fonts.rs) — and loaded before the first frame.
    .font(include_bytes!("../Inter-Light.ttf").as_slice())
    .font(include_bytes!("../Inter_Regular.ttf").as_slice())
    .font(include_bytes!("../JetBrainsMono-Regular.ttf").as_slice())
    .default_font(utils::fonts::SANS)
    .title("Dannesk")
    .window(window_settings())
    .subscription(subscriptions)
    .theme(|state: &AppState| state.theme.clone())
    .run();
    utils::fonts::remove_skip_config();
    ran?;

    handle.block_on(async {
        if let Some(tx) = crate::ws::WS_SHUTDOWN_TX.get() {
            let _ = tx.send(()).await;
        }
        for jh in join_handles {
            let _ = jh.await;
        }
    });

    Ok(())
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    crate::controller::handle_message(state, message)
}

// The stamps are no-ops outside a `startup-trace` build (src/startup_trace.rs).
fn view(state: &AppState) -> iced::Element<'_, Message> {
    startup_trace::stamp("window and GPU up, first view");
    let dashboard = ui::dashboard::render_dashboard(state);
    startup_trace::stamp("first view built");
    dashboard
}

fn subscriptions(state: &AppState) -> Subscription<Message> {
    controller::subscriptions(state)
}

/// X11 draws the embedded icon pixels; Wayland ignores them and resolves the
/// icon through application_id → dannesk.desktop (see src/icon.rs).
fn window_settings() -> iced::window::Settings {
    let mut settings = iced::window::Settings {
        icon: icon::window_icon(),
        ..Default::default()
    };
    #[cfg(target_os = "linux")]
    {
        settings.platform_specific.application_id = icon::APP_ID.to_owned();
    }
    settings
}
