extern crate serde_derive;
use kube::{api::{Api, ListParams, Meta, WatchEvent}, Client};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", kind="Queue", namespaced)]
pub struct QueueSpec {
    pub name: String,
}

#[tokio::main]
async fn main() -> Result<(), kube::Error>  {
    let client = Client::try_default().await?;
    // Set a namespace. We're just hard-coding for now.
    let namespace = "default";

    let crds: Api<CustomResourceDefinition> = Api::namespaced(client, namespace);
    let lp = ListParams::default().fields("apiVersion=tibcoems.apimeister.com").timeout(20);

    let mut stream = crds.watch(&lp, "0").await?.boxed();
    while let Some(status) = stream.try_next().await? {
        match status {
            WatchEvent::Added(s) => println!("Added {}", Meta::name(&s)),
            WatchEvent::Modified(s) => println!("Modified: {}", Meta::name(&s)),
            WatchEvent::Deleted(s) => println!("Deleted {}", Meta::name(&s)),
            WatchEvent::Error(s) => println!("{}", s),
            _ => {}
        }
    }
    Ok(())
}