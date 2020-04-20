use tokio::fs::File;
use tokio::prelude::*;
use serde_derive::Deserialize;
use failure::Fallible;

/// Configuration file representation
#[derive(Deserialize)]
pub struct Config {
    /// The redis section
    pub redis: RedisSection,
}

/// Redis section representation (from the config file)
#[derive(Deserialize)]
pub struct RedisSection {
    /// The prefix to use when storing data in redis
    prefix: Option<String>,
    /// The redis server URL (eg: `redis://:password@redis.domain.tld`
    pub url: String
}

impl RedisSection {
    /// Return the redis prefix to use (with the ending dot)
    pub fn prefix(&self) -> String {
        match &self.prefix {
            // Attach the ending point to the existing prefix and return.
            Some(prefix) => prefix.to_owned()+".",
            // When no prefix is used, just return an empty string.
            _ => String::new(),
        }
    }
}

/// Get the config from file (at `./config.toml`),
/// parse the content and return [`Config`][crate::config::Config].
pub async fn get_config() -> Fallible<Config> {
    // Read file
    let mut conf = File::open("./config.toml").await?;
    let mut contents = vec![];
    conf.read_to_end(&mut contents).await?;

    // Parse file
    let conf : Config = toml::from_slice(&contents)?;
    Ok(conf)
}