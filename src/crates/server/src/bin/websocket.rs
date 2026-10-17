#[tokio::main(worker_threads = 2)]
async fn main() -> std::io::Result<()> {
    if std::env::args().any(|s| s == "--healthcheck") {
        peakrunner_discovery::http::http_get("http://127.0.0.1:8080/healthz", false)?;
        return Ok(());
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let bind = std::env::var("PEAKRUNNER_HTTP_BIND").unwrap_or("127.0.0.1:8080".into());
    let trusted = std::env::var("PEAKRUNNER_TRUST_CLOUDFLARE").as_deref() == Ok("1");
    // Explicit opt-in required for a non-loopback HTTP origin; never publish its port.
    if !bind.parse::<std::net::SocketAddr>().is_ok_and(|a| a.ip().is_loopback()) && !trusted {
        return Err(std::io::Error::other("non-loopback HTTP origin requires trusted Cloudflare mode and an isolated network"));
    }
    peakrunner_server::websocket::serve(&bind, trusted).await
}
