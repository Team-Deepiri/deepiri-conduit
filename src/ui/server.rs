use anyhow::{Context, Result};
use axum::{
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use tracing::info;

use crate::docker;
use crate::proxy;
use crate::registry::state;

async fn page_html() -> Html<String> {
    let conduit_state = state::load().unwrap_or_default();
    let (docker_ok, proxy_status) = match docker::client::connect().await {
        Ok(d) => {
            let p = proxy::manager::get_proxy_status(&d).await.ok().flatten();
            (true, p)
        }
        Err(_) => (false, None),
    };
    let body = super::dashboard::render(&conduit_state, docker_ok, proxy_status.as_deref());
    Html(body)
}

async fn page_json() -> impl IntoResponse {
    let conduit_state = state::load().unwrap_or_default();
    Json(conduit_state)
}

/// Local web dashboard (HTTP only, binds loopback).
pub async fn run_dashboard(port: u16, open_browser: bool) -> Result<()> {
    let app = Router::new()
        .route("/", get(page_html))
        .route("/api/state", get(page_json));

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Bind dashboard to {} (is the port free?)", addr))?;

    let url = format!("http://{}", addr);
    info!("Conduit dashboard at {}", url);
    println!();
    println!("  Dashboard: {}", url);
    println!("  API JSON:  {}/api/state", url);
    println!();
    println!("  Press Ctrl+C to stop.");
    println!();

    if open_browser {
        if let Err(e) = open::that(&url) {
            tracing::warn!("Could not open browser: {}", e);
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("dashboard server")?;

    Ok(())
}
