use {
    log::{info, LevelFilter},
    simple_logger::SimpleLogger,
    std::net::SocketAddr,
    tokio::signal::unix::{signal, SignalKind},
};

mod app;
mod hyper_helpers;
mod resources;
mod service;

use common;
use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    SimpleLogger::new()
        .with_module_level("mio", LevelFilter::Warn)
        .with_module_level("tokio_tungstenite", LevelFilter::Warn)
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();
    info!("Version: {}", common::VERSION);
    let sigint = signal(SignalKind::interrupt()).expect("failed to set up signal handler");
    let app = App::new(sigint);
    app.serve(&SocketAddr::from(([0, 0, 0, 0], 8080))).await?;
    Ok(())
}
