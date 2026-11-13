use std::env;

#[derive(Clone, Debug)]
pub struct Settings {
    pub database_url: String,
    pub database_max_connections: u32,
    pub port: u16,
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_owned())
            .parse()?;
        let database_max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_owned())
            .parse()?;

        Ok(Self {
            database_url,
            database_max_connections,
            port,
        })
    }
}
