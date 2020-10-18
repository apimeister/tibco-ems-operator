use kube::{api::{Api, ListParams}, Client};
use kube_runtime::watcher;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;
use std::env;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use hyper::Result;

#[path = "../ems/mod.rs"]
mod ems;
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

pub static KNOWN_BRIDGES: Lazy<Mutex<HashMap<String, Bridge>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

pub async fn watch_bridges() -> Result<()>{
  let crds: Api<Bridge> = get_bridge_client().await;
  let lp = ListParams::default();

  println!("subscribing events of type bridges.tibcoems.apimeister.com/v1");
  let mut stream = watcher(crds, lp).boxed();
  while let Some(status) = stream.try_next().await.unwrap() {
    match status {
      kube_runtime::watcher::Event::Applied(bridge) =>{
        create_bridge(bridge);
      }
      kube_runtime::watcher::Event::Deleted(bridge) =>{
        delete_bridge(bridge);
      },
      kube_runtime::watcher::Event::Restarted(_bridge) =>{
        println!("restart bridge event not implemented");
      },
    }
  }
  println!("finished watching bridges");
  Ok(())
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