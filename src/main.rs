#[tokio::main]
async fn main() -> color_eyre::eyre::Result<()> {
    sape::run().await
}
