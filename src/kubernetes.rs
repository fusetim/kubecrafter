use std::collections::HashMap;

use failure::Fallible;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::ListParams;

use crate::KClient;

#[derive(Clone, Debug, Default)]
pub struct Server {
    pub name: String,
    pub namespace: Option<String>,
    pub set: Option<String>,
    pub fqdn: Option<String>,
}


pub async fn get_pods(kclient: KClient, namespace: String) -> Fallible<Vec<Pod>> {
    info!(r#"Retrieving pods in namespace "{}"..."#, namespace);

    let api: Api<Pod> = Api::namespaced(kclient, &namespace);
    let pods = api.list(&ListParams::default()).await?.items;
    info!(r#"Retrieved pods in namespace "{}"!"#, namespace);
    Ok(pods)
}

pub async fn get_servers_info(pods: &Vec<Pod>) -> Fallible<Vec<Server>> {
    let infos = pods.iter().map(|pod| {
        let mut name = "unknown".to_owned();
        let mut fqdn = None;
        let mut namespace: Option<String> = None;
        let mut set = None;
        if let Some(spec) = &pod.spec {
            if let Some(hostname) = &spec.hostname {
                if let Some(subdomain) = &spec.subdomain {
                    fqdn = Some(hostname.to_owned() + "." + &subdomain);
                }
            }
        }
        if let Some(md) = &pod.metadata {
            if let Some(kname) = &md.name {
                name = kname.clone();
            };
            if let Some(dn) = &fqdn {
                if let Some(ns) = &md.namespace {
                    fqdn = Some(dn.to_owned() + "." + ns);
                    namespace = Some(ns.clone());
                };
            };
            if let Some(owners) = &md.owner_references {
                if let Some(owner) = owners.get(0) {
                    set = Some(owner.name.clone());
                }
            }
        };
        Server {
            name,
            namespace,
            fqdn,
            set,
        }
    }).collect();
    Ok(infos)
}

pub async fn split_by_ns(servers: Vec<Server>, namespaces: &Vec<String>) -> Fallible<HashMap<String, Vec<Server>>> {
    let mut ns_pods: HashMap<String, Vec<Server>> = HashMap::new();
    for server in servers {
        if let Some(ns) = &server.namespace {
            if !namespaces.contains(ns) {
                debug!(r#"Pod "{}" (from namespace: "{}") eliminated"#, server.name, ns);
                continue;
            }
            match ns_pods.get_mut(ns) {
                Some(ns) => ns.push(server),
                None => { ns_pods.insert(ns.clone(), vec![server]); }
            }
        } else {
            debug!(r#"Pod "{}" eliminated!"#, server.name)
        }
    }
    Ok(ns_pods)
}

pub async fn split_by_set(servers: Vec<Server>, sets: &Vec<String>) -> Fallible<HashMap<String, Vec<Server>>> {
    let mut set_pods: HashMap<String, Vec<Server>> = HashMap::new();
    for server in servers {
        if let Some(set) = &server.set {
            if !sets.contains(set) {
                debug!(r#"Pod "{}" (from set: "{}") eliminated"#, server.name, set);
                continue;
            }
            match set_pods.get_mut(set) {
                Some(set) => set.push(server),
                None => { set_pods.insert(set.clone(), vec![server]); }
            }
        } else {
            debug!(r#"Pod "{}" eliminated!"#, server.name)
        }
    }
    Ok(set_pods)
}

