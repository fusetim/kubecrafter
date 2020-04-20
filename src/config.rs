use tokio::fs::File;
use tokio::prelude::*;
use serde_derive::Deserialize;
use failure::Fallible;

/// Configuration file representation
#[derive(Deserialize)]
pub struct Config {
    /// The redis section
    pub redis: RedisSection,
    /// The namespaces section
    pub namespace: Vec<Namespace>,
}

/// Redis section representation (from the config file)
#[derive(Deserialize)]
pub struct RedisSection {
    /// The prefix to use when storing data in redis
    prefix: Option<String>,
    /// The redis server URL (eg: `redis://:password@redis.domain.tld`
    pub url: String
}

/// Namespace declaration (from the config file)
#[derive(Deserialize)]
pub struct Namespace {
    /// The kubernetes name
    kname: String,
    /// The StatefulSets
    set: Vec<Set>,
}

/// Set declaration (from the config file)
#[derive(Deserialize)]
pub struct Set {
    /// The kubernetes name
    kname: String,
    /// Is this set, a proxy server (like Bungeecord)
    proxy: bool,
    /// Number of server already started outside of this cluster (like on a VPS/Dedicated Server)
    already_connected: u32,
    /// The maximum player on one instance
    players_per_server: u32,
    /// The strategies to use to scale up/down the set
    strategies: Vec<Strategy>,
}

impl Set {
    /// Determine the number of replicas needed for a set
    pub fn get_replicas(&self, proxy_player: u32, set_players: u32) -> u32 {
        // Determines the number of backup servers required according to the current strategy
        let backup = self.strategies.iter()
            .find(|s | s.is_applied(proxy_player, set_players))
            .map(|s| s.backup_server)
            .unwrap_or(1u32);
        // Calculate the minimum number of servers needed to support the current load
        let needed = (set_players as f64 / (self.players_per_server as f64)).ceil() as u32;
        // Calculate the number of replicas required, taking into account non-cluster servers and backup replicas.
        needed+backup-self.already_connected
    }
}

/// Strategy declaration (from the config file)
/// Determines the number of backup servers to launch according to the number of players in the set and the proxies.
#[derive(Deserialize)]
pub struct Strategy {
    /// The threshold (min) of players connected to the proxies from which to activate the strategy
    min_proxy_player: Option<u32>,
    /// The threshold (max) of players connected to the proxies from which to deactivate the strategy
    max_proxy_player: Option<u32>,
    /// The threshold (min) of players connected to the set from which to activate the strategy
    min_player: Option<u32>,
    /// The threshold (max) of players connected to the set from which to deactivate the strategy
    max_player: Option<u32>,
    /// The number of backup pods/servers to launch when the strategy is enabled
    pub backup_server: u32,
}

impl Strategy {
    /// Determines whether the strategy should be applied.
    pub fn is_applied(&self, proxy_players: u32, set_players: u32) -> bool {
        let minp = self.min_proxy_player.unwrap_or(0);
        let maxp = self.max_proxy_player.unwrap_or(u32::max_value());
        let min = self.min_player.unwrap_or(0);
        let max = self.max_player.unwrap_or(u32::max_value());

        // Check that there are the necessary players to activate the strategy.
        minp <= proxy_players &&  proxy_players <= maxp && min <= set_players && set_players <= max
    }
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