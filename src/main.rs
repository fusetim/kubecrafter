#[macro_use]
extern crate log;
extern crate tokio;

use std::collections::HashMap;

use failure::{Causes, err_msg, Error, Fail, Fallible};
use failure::ResultExt;
use future::join_all;
use futures::{FutureExt, TryFutureExt};
use futures::join;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use redis::AsyncCommands;
use regex::Regex;

use config::Config;
use kubernetes::{get_pods, get_servers_info};

use crate::config::Set;
use crate::kubernetes::{Server, split_by_set};

mod config;
mod kubernetes;

type KClient = kube::Client;
type RConnection = redis::aio::Connection;

#[tokio::main]
async fn main() -> Fallible<()> {
    // Init logger
    env_logger::init();
    info!("Hello, world!");

    // Get the config
    info!("Retrieve the config...");
    let conf: Config = config::get_config().await?;

    // Will connect to redis and test the connection
    let redis = connect_redis(&conf.redis.url)
        .and_then(|mut con| async {
            test_redis(&mut con, (&conf.redis.prefix()).clone()).await?;
            Ok(con)
        }).map_err(Error::from);

    // Will make a connection to the kubernetes cluster
    let kube = KClient::try_default().map_err(Error::from);
    let rcon_pass = std::env::var("RCON_PASSWORD").expect("The RCON_PASSWORD env_var is required!");
    let servers = kube.and_then(|client| get_servers(client, conf.clone())).map_err(Error::from);

    // Await the futures and makes the connections
    let conns = join!(servers, redis);
    let servers: HashMap<String, Vec<Server>> = conns.0.map_err(print_error).unwrap();
    let redis: RConnection = conns.1.map_err(print_error).unwrap();

    for (set, servers) in servers.iter() {
        let mut total_set = 0usize;
        for server in servers {
            let players = count_players(server.fqdn.as_ref().unwrap(), &rcon_pass).map_err(print_error).await.unwrap();
            total_set += players;
            debug!(r#"Found {} players in server "{}" (set: {})"#, players, server.name, set);
        }
        debug!(r#"Found {} players in the set "{}""#, total_set, set);
    }

// Exit
    info!("Bye!");
    Ok(())
}

/// Make an async connection to the redis server at the given url.
async fn connect_redis(redis_url: &str) -> Fallible<RConnection> {
    info!(r#"Connection to "{}"..."#, redis_url);
// Make a Redis client to connect
    let client = redis::Client::open(redis_url)
        .context(r#"URL invalid in "REDIS_URL" env_var!"#)?;

// Open an async connection
    let con: RConnection = client.get_async_connection().await
        .context("Connection to redis failed!")?;
    info!("Connected to redis!");
    Ok(con)
}

/// Test the redis connection by adding 1 to the `prefix.hello` key
/// Will be deleted in the future
async fn test_redis(con: &mut RConnection, prefix: String) -> Fallible<()> {
    debug!("Testing redis connection...");
// Determine the key to test
    let key: String = prefix + "hello";
// Get the current value (or 0)
    let mut hello = con.get::<&str, usize>(&key).await.unwrap_or(0);
// Print value and increment
    debug!("{}: {}", key, hello);
    hello += 1;
// Set the key with the new value
    con.set::<&str, usize, ()>(&key, hello).await?;
    debug!("Connection test to redis passed!");
    Ok(())
}

async fn count_players(address: &str, password: &str) -> Fallible<usize> {
    let conn = rcon::Connection::connect(address, password);
    conn.map_err(Error::from)
        .and_then(|mut conn| async move {
            let players = conn.cmd("list").await?;
            let regex = Regex::new(r#"^There are (\d+) of a max \d+ players online: $"#)?;
            match regex.captures(&players) {
                Some(capts) => {
                    match capts.get(1) {
                        Some(count) => Ok(count.as_str().parse::<usize>()?),
                        None => Err(err_msg("Information unavailable! Not mentioned in the command /list")),
                    }
                }
                None => Err(err_msg("Information unavailable! Not mentioned in the command /list")),
            }
        }).await
}

async fn get_servers(client: KClient, conf: Config) -> Fallible<HashMap<String, Vec<Server>>> {
    let sets_names: Vec<String> = (&conf.namespace).into_iter()
        .map(|ns| &ns.set)
        .flatten()
        .map(|set: &Set| set.kname.clone())
        .collect();
    let futures: FuturesUnordered<_> = conf.namespace.into_iter()
        .map(|ns| get_pods(client.clone(), ns.kname))
        .collect();
    let stream: Vec<Fallible<_>> = futures.into_stream()
        .map(|fut| async {
            let sets_names = sets_names.clone();
            let pods = fut.unwrap();
            get_servers_info(&pods)
                .and_then(|servers| split_by_set(servers, &sets_names))
                .await
        }).buffer_unordered(3)
        .collect().await;
    let result: Fallible<Vec<HashMap<String, Vec<Server>>>> = stream.into_iter().collect();
    let map: HashMap<String, Vec<Server>> = result?.into_iter()
        .fold(HashMap::new(), |mut map, set| {
            map.extend(set);
            map
        });
    Ok(map)
}

// Print error, causes and backtrace in the log
fn print_error(err: Error) {
// Print the error
    error!("{}", err);
// Print the causes
    let causes: Causes = err.iter_causes();
    causes.enumerate().for_each(|(index, fail)| {
        error!("causes [{}]: {}", index + 1, fail)
    });
// If backtrace is enabled and not empty, print it.
    if !err.backtrace().is_empty() {
        error!("backtrace:\n{}", err.backtrace());
    }
}