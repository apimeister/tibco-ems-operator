extern crate serde_derive;
use kube::{api::{Api, ListParams, Meta, WatchEvent}, Client};
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", kind="Queue", namespaced)]
pub struct QueueSpec {
    pub name: String,
    pub global: Option<bool>,
    pub max_msg: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<(), kube::Error>  {
    println!("infering client config");
    let config = Config::infer().await;
    println!("infering client config done");
    match config {
        Err(e) => println!("error {}", e),
        Ok(c) => {
            println!("success ");
            let client = Client::new(c);
            println!("initializing client");
            //config.accept_invalid_certs = true;
            // let client = Client::try_default().await?;
            // Set a namespace. We're just hard-coding for now.
            let namespace = "default";

            println!("connecting");
            let crds: Api<Queue> = Api::namespaced(client, namespace);
            let lp = ListParams::default();

            println!("streaming events ...");
            let mut stream = crds.watch(&lp, "0").await?.boxed();
            while let Some(status) = stream.try_next().await? {
                match status {
                    WatchEvent::Added(queue) => println!("Added {}", Meta::name(&queue)),
                    WatchEvent::Modified(queue) => {
                        println!("Modified: {}", Meta::name(&queue));
                        println!("{}",queue.spec.name);
                        match queue.spec.global{
                            Some(g) => println!("isGlobal: {}",g),
                            None => println!("global not set"),
                        }
                        let json_str = serde_json::to_string_pretty( &queue);
                        println!("{}",json_str.unwrap());
                    },
                    WatchEvent::Deleted(queue) => println!("Deleted {}", Meta::name(&queue)),
                    WatchEvent::Error(queue) => println!("error: {}", queue),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}