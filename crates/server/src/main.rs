#[tokio::main]
async fn main() -> anyhow::Result<()> {
    chef_engine::command::run().await
}
