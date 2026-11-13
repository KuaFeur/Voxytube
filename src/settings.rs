use std::env;

#[derive(Clone, Debug)]
pub struct Settings {
    pub database_url: String,
    pub database_max_connections: u32,
    pub port: u16,
    pub callback_url: String,
    pub api_token: String,
    pub voxy_webhook_url: String,
    pub voxy_webhook_token: Option<String>,
    pub rss_interval_seconds: u64,
    pub hub_url: String,
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
        let port = env::var("PORT").unwrap_or_else(|_| "8080".into()).parse()?;
        let database_max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".into()).parse()?;
        let callback_url = env::var("PUBLIC_CALLBACK_URL")
            .map_err(|_| anyhow::anyhow!("PUBLIC_CALLBACK_URL must be set (public HTTPS URL ending in /websub)"))?;
        if !callback_url.starts_with("https://") || !callback_url.ends_with("/websub") {
            anyhow::bail!("PUBLIC_CALLBACK_URL must be HTTPS and end with /websub");
        }
        let api_token = env::var("API_TOKEN")
            .map_err(|_| anyhow::anyhow!("API_TOKEN must be set to protect the management API"))?;
        if api_token.len() < 32 {
            anyhow::bail!("API_TOKEN must contain at least 32 characters");
        }
        let voxy_webhook_url = env::var("VOXY_WEBHOOK_URL").unwrap_or_default();
        let voxy_webhook_token = env::var("VOXY_WEBHOOK_TOKEN").ok().filter(|v| !v.is_empty());
        let rss_interval_seconds = env::var("RSS_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "300".into()).parse()?;
        let hub_url = env::var("WEBSUB_HUB_URL")
            .unwrap_or_else(|_| "https://pubsubhubbub.appspot.com/subscribe".into());

        Ok(Self {
            database_url, database_max_connections, port, callback_url, api_token,
            voxy_webhook_url, voxy_webhook_token, rss_interval_seconds, hub_url,
        })
    }
}
