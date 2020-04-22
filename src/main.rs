#[macro_use]
extern crate log;
extern crate tokio;

use std::collections::HashMap;
use failure::{Causes, Error, Fallible};
use failure::ResultExt;
use futures::{FutureExt, TryFutureExt};
use futures::join;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use redis::AsyncCommands;
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
        });

    // Will make a connection to the kubernetes cluster
    let kube = KClient::try_default().map_err(Error::from);
    let nss = conf.namespace.clone();
    let sets_names: Vec<String> = (&conf.namespace).into_iter().map(|ns| &ns.set).flatten().map(|set: &Set| set.kname.clone()).collect();
    let servers = kube
        .and_then(|client: KClient| {
            async move {
                let futures: FuturesUnordered<_> = FuturesUnordered::new();
                for ns in nss {
                    futures.push(get_pods(client.clone(), ns.kname))
                }
                let stream = futures.into_stream();
                let vec: Vec<Fallible<_>> = stream.map(|fut| async {
                    let pods = fut.unwrap();
                    get_servers_info(&pods)
                        .and_then(|servers| split_by_set(servers, &sets_names))
                        .and_then(|namespaces: HashMap<String, Vec<Server>>| async {
                            namespaces.into_iter().for_each(|(ns, servers)| {
                                servers.into_iter().for_each(|server| {
                                    debug!(r#"Found server "{}" for set "{}"!"#, server.name, ns);
                                })
                            });
                            Ok(())
                        }).await
                }).buffer_unordered(3)
                    .collect().await;
                let fail: Fallible<Vec<_>> = vec.into_iter().collect();
                Ok(fail)
            }
        }).map(Fallible::from);

    // Await the futures and makes the connections
    let conns = join!(servers, redis);
    let servers: Fallible<_> = conns.0;
    let redis: Fallible<RConnection> = conns.1;
    redis.map_err(Error::from).map_err(print_error);
    servers.map_err(print_error);

// TODO...

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