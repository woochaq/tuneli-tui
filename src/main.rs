pub mod models;
pub mod engine;
pub mod telemetry;
pub mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut app = ui::app::App::new().await;
    app.run().await?;
    Ok(())
}
