mod actions;
mod agent;
mod apps;
mod hints;
mod models;
mod service;

use anyhow::Result;
use copilot_core::{BUS_NAME, OBJECT_PATH};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "copilotd=info".into()))
        .init();

    let _conn = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service::Service::new())?
        .build()
        .await?;
    tracing::info!("listening on {BUS_NAME}");

    tokio::signal::ctrl_c().await.ok();
    Ok(())
}
