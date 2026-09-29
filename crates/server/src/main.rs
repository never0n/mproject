use mortown_server::Server;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mortown:mortown_local_dev@127.0.0.1:55432/mortown".into());
    let address = std::env::var("MORTOWN_LISTEN").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let (stop, receiver) = watch::channel(false);
    let server = Server::new(&url, receiver.clone()).await?;
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!("Мортаун: сервер слушает {address}, PostgreSQL подключена, симуляция 20 Гц");
    let mut stopped = receiver.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        stop.send_replace(true);
    });
    axum::serve(
        listener,
        server
            .router()
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = stopped.changed().await;
    })
    .await?;
    for _ in 0..100 {
        if server.simulation.lock().await.snapshot().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    server.pool.close().await;
    Ok(())
}
