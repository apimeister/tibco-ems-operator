use kube::{api::{Api, ListParams, Meta}, Client};
use kube::api::WatchEvent;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;
use std::env;
use hyper::Result;
use schemars::JsonSchema;

use crate::ems;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug, JsonSchema)]
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
  let lp = ListParams::default();

  let mut last_version: String = "0".to_owned();
  info!("subscribing events of type bridges.tibcoems.apimeister.com/v1");
  loop{
    debug!("B: new loop iteration with offset {}",last_version);
    let mut stream = crds.watch(&lp, &last_version).await.unwrap().boxed();
    loop {
      debug!("B: new stream item");
      let stream_result = stream.try_next().await;
      match stream_result {
        Ok(status_obj) => {
          match status_obj {
            Some(status) => {
              match status {
                WatchEvent::Added(bridge) => {
                  let ver = Meta::resource_ver(&bridge).unwrap();
                  info!("Added {}@{}", Meta::name(&bridge), &ver);
                  create_bridge(bridge);
                  last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
                },
                WatchEvent::Modified(bridge) => {
                  let ver = Meta::resource_ver(&bridge).unwrap();
                  info!("Modified {}@{}", Meta::name(&bridge), &ver);
                  create_bridge(bridge);
                  last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
                }
                WatchEvent::Deleted(bridge) => {
                  let ver = Meta::resource_ver(&bridge).unwrap();
                  info!("Deleted {}@{}", Meta::name(&bridge), &ver);
                  delete_bridge(bridge);
                  last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
                },
                WatchEvent::Error(e) => {
                  error!("Error {}", e);
                  error!("resetting offset to 0");
                  last_version="0".to_owned();
                },
                _ => {}
              };
            },
            None => {
              debug!("B: request loop returned empty");  
              break;
            }
          }
        },
        Err(err) => {
          debug!("B: error on request loop {:?}",err);  
          break;
        }
      }
    }
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
  info!("creating bridge {}",bridge_name);
  let mut source_name = bridge.spec.source_name;
  let mut target_name = bridge.spec.target_name;
  source_name.make_ascii_uppercase();
  target_name.make_ascii_uppercase();
  let script = "create bridge ".to_owned()
        +"source="+&bridge.spec.source_type+":"+&source_name
        +" target="+&bridge.spec.target_type+":"+&target_name
        +&"\n";
  info!("script: {}",script);
  let result = ems::run_tibems_script(script);
  info!("{}",result);
}

fn delete_bridge(bridge: Bridge){
  let bridge_name: String = bridge.metadata.name.unwrap();
  info!("deleting bridge {}",bridge_name);
  let mut source_name = bridge.spec.source_name;
  let mut target_name = bridge.spec.target_name;
  source_name.make_ascii_uppercase();
  target_name.make_ascii_uppercase();
  let script = "delete bridge ".to_owned()
        +"source="+&bridge.spec.source_type+":"+&source_name
        +" target="+&bridge.spec.target_type+":"+&target_name
        +&"\n";
  info!("script: {}",script);
  let result = ems::run_tibems_script(script);
  info!("{}",result);
}