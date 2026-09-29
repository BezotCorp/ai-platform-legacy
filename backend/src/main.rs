mod agents;
mod configurations;
mod conversation;
mod file_manager;
mod io;
mod providers;
mod sessions;
mod sqlite;
mod tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    io::run().await
}
