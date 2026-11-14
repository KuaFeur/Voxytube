use std::env;

#[derive(Clone, Debug)]
pub struct Settings {
    pub database_url: String,
    pub database_max_connections: u32,
    pub port: u16,
    pub callback_url: String,
    pub api_token: String,
    pub webhook_url: String,
    pub webhook_token: Option<String>,
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
        let callback = reqwest::Url::parse(&callback_url)
            .map_err(|_| anyhow::anyhow!("PUBLIC_CALLBACK_URL must be a valid URL"))?;
        if callback.scheme() != "https" || callback.host_str().is_none()
            || callback.path() != "/websub" || callback.query().is_some()
            || callback.fragment().is_some() || !callback.username().is_empty()
            || callback.password().is_some() {
            anyhow::bail!("PUBLIC_CALLBACK_URL must be an HTTPS URL ending exactly in /websub, without credentials, query, or fragment");
        }
        let api_token = env::var("API_TOKEN")
            .map_err(|_| anyhow::anyhow!("API_TOKEN must be set to protect the management API"))?;
        if api_token.len() < 32 {
            anyhow::bail!("API_TOKEN must contain at least 32 characters");
        }
        let webhook_url = env::var("WEBHOOK_URL").unwrap_or_default();
        let webhook_token = env::var("WEBHOOK_TOKEN").ok().filter(|v| !v.is_empty());
        let rss_interval_seconds: u64 = env::var("RSS_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "300".into()).parse()?;
        if !(60..=86_400).contains(&rss_interval_seconds) {
            anyhow::bail!("RSS_INTERVAL_SECONDS must be between 60 and 86400");
        }
        if !(1..=100).contains(&database_max_connections) {
            anyhow::bail!("DATABASE_MAX_CONNECTIONS must be between 1 and 100");
        }
        let hub_url = env::var("WEBSUB_HUB_URL")
            .unwrap_or_else(|_| "https://pubsubhubbub.appspot.com/subscribe".into());
        let hub = reqwest::Url::parse(&hub_url)
            .map_err(|_| anyhow::anyhow!("WEBSUB_HUB_URL must be a valid URL"))?;
        if hub.scheme() != "https" || hub.host_str().is_none() || !hub.username().is_empty() || hub.password().is_some() {
            anyhow::bail!("WEBSUB_HUB_URL must be an HTTPS URL without credentials");
        }
        if !webhook_url.is_empty() {
            let webhook = reqwest::Url::parse(&webhook_url)
                .map_err(|_| anyhow::anyhow!("WEBHOOK_URL must be a valid URL"))?;
            if !matches!(webhook.scheme(), "https" | "http") || webhook.host_str().is_none()
                || !webhook.username().is_empty() || webhook.password().is_some() {
                anyhow::bail!("WEBHOOK_URL must be an HTTP(S) URL without credentials");
            }
        }

        Ok(Self {
            database_url, database_max_connections, port, callback_url, api_token,
            webhook_url, webhook_token, rss_interval_seconds, hub_url,
        })
    }
}
