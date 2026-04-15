mod cache;
mod config;
mod docs;
mod error;
mod server;
mod service;

mod proto {
    tonic::include_proto!("pp.v1");
}

use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use tokio::signal;
use tonic::transport::Server;
use tracing::info;

use crate::config::Config;
use crate::proto::performance_service_server::PerformanceServiceServer;
use crate::server::PerformanceServiceImpl;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::from_env()?;
    config.init_tracing();

    let addr: SocketAddr = config.addr.parse()?;

    info!(version = env!("CARGO_PKG_VERSION"), %addr, "starting");

    // tonic::Server::builder().into_router() is marked deprecated but has no replacement yet
    #[allow(deprecated)]
    let grpc = Server::builder()
        .add_service(PerformanceServiceServer::new(PerformanceServiceImpl))
        .into_router();

    let http = Router::new()
        .route("/health", get(health))
        .route("/ready", get(|| async { "ok" }))
        .route("/live", get(|| async { "ok" }))
        .route("/docs", get(docs_handler));

    let app = Router::new().merge(http).merge(grpc);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("shutdown complete");
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    let cache = crate::cache::stats();
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "cache": cache
    }))
}

async fn docs_handler() -> axum::response::Html<String> {
    axum::response::Html(render_markdown(crate::docs::DOCUMENTATION))
}

fn render_markdown(md: &str) -> String {
    use std::fmt::Write;
    let mut html = String::from(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rosu-pp-service docs</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; line-height: 1.6; max-width: 900px; margin: 0 auto; padding: 2rem; color: #24292f; background: #fff; }
h1, h2, h3 { border-bottom: 1px solid #d0d7de; padding-bottom: 0.3em; margin-top: 1.5em; }
h1 { font-size: 2em; }
h2 { font-size: 1.5em; }
code { background: #f6f8fa; padding: 0.2em 0.4em; border-radius: 6px; font-size: 85%; }
pre { background: #161b22; color: #c9d1d9; padding: 1rem; border-radius: 6px; overflow-x: auto; }
pre code { background: none; padding: 0; color: inherit; }
table { border-collapse: collapse; width: 100%; margin: 1em 0; }
th, td { border: 1px solid #d0d7de; padding: 0.5em 1em; text-align: left; }
th { background: #f6f8fa; }
hr { border: none; border-top: 1px solid #d0d7de; margin: 2em 0; }
a { color: #0969da; text-decoration: none; }
a:hover { text-decoration: underline; }
blockquote { border-left: 4px solid #d0d7de; margin: 0; padding-left: 1em; color: #57606a; }
</style>
</head>
<body>
"#);

    let mut in_code_block = false;
    let mut in_table = false;

    for line in md.lines() {
        if line.starts_with("```") {
            if in_code_block {
                html.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                html.push_str("<pre><code>");
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            let escaped = line.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            let _ = writeln!(html, "{}", escaped);
            continue;
        }

        if line.starts_with('|') && line.ends_with('|') {
            if line.contains("---") {
                continue;
            }
            let is_header = !in_table;
            if !in_table {
                html.push_str("<table>\n");
                in_table = true;
            }
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(|s| s.trim()).collect();
            html.push_str("<tr>");
            let tag = if is_header { "th" } else { "td" };
            for cell in cells {
                let _ = write!(html, "<{}>{}</{}>", tag, escape_html(cell), tag);
            }
            html.push_str("</tr>\n");
            continue;
        } else if in_table {
            html.push_str("</table>\n");
            in_table = false;
        }

        if line.starts_with("# ") {
            let _ = writeln!(html, "<h1>{}</h1>", escape_html(&line[2..]));
        } else if line.starts_with("## ") {
            let _ = writeln!(html, "<h2>{}</h2>", escape_html(&line[3..]));
        } else if line.starts_with("### ") {
            let _ = writeln!(html, "<h3>{}</h3>", escape_html(&line[4..]));
        } else if line.starts_with("---") {
            html.push_str("<hr>\n");
        } else if line.starts_with("- ") || line.starts_with("* ") {
            let _ = writeln!(html, "<li>{}</li>", process_inline(&line[2..]));
        } else if line.is_empty() {
            html.push_str("<br>\n");
        } else {
            let _ = writeln!(html, "<p>{}</p>", process_inline(line));
        }
    }

    if in_table {
        html.push_str("</table>\n");
    }

    html.push_str("</body></html>");
    html
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn process_inline(s: &str) -> String {
    let mut result = escape_html(s);

    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let code = &result[start + 1..start + 1 + end];
            result = format!(
                "{}<code>{}</code>{}",
                &result[..start],
                code,
                &result[start + 2 + end..]
            );
        } else {
            break;
        }
    }

    while let Some(start) = result.find("**") {
        if let Some(end) = result[start + 2..].find("**") {
            let bold = &result[start + 2..start + 2 + end];
            result = format!(
                "{}<strong>{}</strong>{}",
                &result[..start],
                bold,
                &result[start + 4 + end..]
            );
        } else {
            break;
        }
    }

    result
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received SIGINT"),
        () = terminate => info!("received SIGTERM"),
    }
}
