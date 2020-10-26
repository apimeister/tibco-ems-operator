use kube::{api::{Api, ListParams, Meta}, Client};
use kube::api::WatchEvent;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;
use std::env;
use hyper::Result;

use crate::ems;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", kind="Bridge", namespaced)]
#[allow(non_snake_case)]
pub struct BridgeSpec {
  pub source_type: String,
  pub source_name: String,
  pub target_type: String,
  pub target_name: String,
  pub selector: Option<String>,
}

pub async fn watch_bridges() -> Result<()>{
  let crds: Api<Bridge> = get_bridge_client().await;
  let lp = ListParams::default().timeout(30);

  let mut last_version: String = "0".to_owned();
  println!("subscribing events of type bridges.tibcoems.apimeister.com/v1");
  loop{
    let mut stream = crds.watch(&lp, &last_version).await.unwrap().boxed();
    while let Some(status) = stream.try_next().await.unwrap() {
      match status {
          WatchEvent::Added(bridge) =>{
            let ver = Meta::resource_ver(&bridge).unwrap();
            println!("Added {}@{}", Meta::name(&bridge), &ver);
            create_bridge(bridge);
            last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
          },
          WatchEvent::Modified(bridge) => {
            let ver = Meta::resource_ver(&bridge).unwrap();
            println!("Modified {}@{}", Meta::name(&bridge), &ver);
            create_bridge(bridge);
            last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
          }
          WatchEvent::Deleted(bridge) =>{
            let ver = Meta::resource_ver(&bridge).unwrap();
            println!("Deleted {}@{}", Meta::name(&bridge), &ver);
            delete_bridge(bridge);
            last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
          },
          WatchEvent::Error(e) => {
            println!("Error {}", e);
            println!("resetting offset to 0");
            last_version="0".to_owned();
          },
          _ => {}
      }
    }
    println!("finished watching bridges");
  }
}

async fn get_bridge_client() -> Api<Bridge>{
  let config = Config::infer().await.unwrap();
  let client: kube::Client = Client::new(config);
  let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
  let crds: Api<Bridge> = Api::namespaced(client, &namespace);
  return crds;
}

fn create_bridge(bridge: Bridge){
  let bridge_name: String = bridge.metadata.name.unwrap();
  println!("creating bridge {}",bridge_name);
  let mut source_name = bridge.spec.source_name;
  let mut target_name = bridge.spec.target_name;
  source_name.make_ascii_uppercase();
  target_name.make_ascii_uppercase();
  let script = "create bridge ".to_owned()
        +"source="+&bridge.spec.source_type+":"+&source_name
        +"target="+&bridge.spec.target_type+":"+&target_name
        +&"\n";
  println!("script: {}",script);
  let result = ems::run_tibems_script(script);
  println!("{}",result);
}

fn delete_bridge(bridge: Bridge){
  let bridge_name: String = bridge.metadata.name.unwrap();
  println!("deleting bridge {}",bridge_name);
  let mut source_name = bridge.spec.source_name;
  let mut target_name = bridge.spec.target_name;
  source_name.make_ascii_uppercase();
  target_name.make_ascii_uppercase();
  let script = "delete bridge ".to_owned()
        +"source="+&bridge.spec.source_type+":"+&source_name
        +"target="+&bridge.spec.target_type+":"+&target_name
        +&"\n";
  println!("script: {}",script);
  let result = ems::run_tibems_script(script);
  println!("{}",result);
}