use std::env;

use tracing::Level;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug)]
pub struct Config {
    pub addr: String,
    pub log_json: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            addr: env::var("PP_SERVICE_ADDR").unwrap_or_else(|_| "[::]:50051".into()),
            log_json: env::var("PP_SERVICE_LOG_FORMAT").map_or(false, |v| v == "json"),
        })
    }

    pub fn init_tracing(&self) {
        let filter = EnvFilter::builder()
            .with_default_directive(Level::INFO.into())
            .from_env_lossy();

        if self.log_json {
            fmt().json().with_env_filter(filter).init();
        } else {
            fmt().with_env_filter(filter).init();
        }
    }
}
