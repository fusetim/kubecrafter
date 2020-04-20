#[macro_use]
extern crate log;
extern crate tokio;

use failure::{Fallible, Fail, Causes, Error};
use failure::ResultExt;
use futures::prelude::*;
use redis::AsyncCommands;
use futures::join;
use crate::config::Config;
use kube::Api;
use kube::api::ListParams;
use k8s_openapi::api::core::v1::Pod;
use futures::future::err;

mod config;

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
    let kube = connect_kubernetes();

    // Await the futures and makes the connections
    let conns = join!(kube, redis);
    let redis : Fallible<RConnection> = conns.1;
    redis.map_err(Error::from).map_err(print_error);

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
    debug!("Test successful!");
    Ok(())
}

/// Make a connection to the Kubernetes namespaces [WIP]
async fn connect_kubernetes() -> Fallible<()> {
    info!("Connection to Kubernetes' clusters");
    // Make a Kubernetes client to connect (from env_var or file at `$HOME/.kube/config`)
    let kube_client = KClient::try_default().await?;
    // Get the API for Pods from the namespace
    // TODO: Remove the hardcoded test value
    let api : Api<Pod> = Api::namespaced(kube_client, "namespace-1");
    // Get the pod list (and start a connection)
    let _pods = api.list(&ListParams::default()).await?;
    info!("Connected to Kubernetes' clusters. Got pod list.");
    Ok(())
}

// Print error, causes and backtrace in the log
fn print_error(err: Error) {
    // Print the error
    error!("{}", err);
    // Print the causes
    let causes: Causes = err.iter_causes();
    causes.enumerate().for_each(| (index, fail)| {
        error!("causes [{}]: {}",index+1, fail)
    });
    // If backtrace is enabled and not empty, print it.
    if !err.backtrace().is_empty() {
        error!("backtrace:\n{}", err.backtrace());
    }
}