use env_var::env_var;
use futures::{StreamExt, TryStreamExt};
use kube::{api::{Api, ListParams, ResourceExt}, Client};
use kube::api::WatchEvent;
use kube_derive::CustomResource;
use hyper::Result;
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tibco_ems::Session;
use tibco_ems::admin::BridgeInfo;
use tibco_ems::Destination;
use std::sync::Mutex;
use once_cell::sync::Lazy;

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

pub static KNOWN_BRIDGES: Lazy<Mutex<HashMap<String, Bridge>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

///used for sending admin operations
static ADMIN_CONNECTION: Lazy<Mutex<Session>> = Lazy::new(|| Mutex::new(super::init_admin_connection()));

pub async fn watch_bridges() -> Result<()>{
  let crds: Api<Bridge> = get_bridge_client().await;
  let lp = ListParams::default();

  let mut last_version: String = "0".to_owned();
  info!("subscribing events of type bridges.tibcoems.apimeister.com/v1");
  loop{
    debug!("new loop iteration with offset {}",last_version);
    let watch_result = crds.watch(&lp, &last_version).await;
    match watch_result {
      Ok(str_result) => {
        let mut stream = str_result.boxed();
        loop {
          debug!("new stream item");
          let stream_result = stream.try_next().await;
          match stream_result {
            Ok(status_obj) => {
              match status_obj {
                Some(status) => {
                  match status {
                    WatchEvent::Added(bridge) => {
                      let bname = ResourceExt::name(&bridge);
                      {
                        let mut res = KNOWN_BRIDGES.lock().unwrap();
                        match res.get(&bname) {
                          Some(_bridge) => debug!("bridge already known {}", &bname),
                          None => {
                            info!("Added {}", bname);
                            create_bridge(&bridge);
                            let b = bridge.clone();
                            let n = bname.clone();
                            res.insert(n, b);
                          },
                        }
                      }
                      last_version = ResourceExt::resource_version(&bridge).unwrap();
                    },
                    WatchEvent::Modified(bridge) => {
                      info!("Modified {}", ResourceExt::name(&bridge));
                      create_bridge(&bridge);
                      last_version = ResourceExt::resource_version(&bridge).unwrap();          
                    }
                    WatchEvent::Deleted(bridge) => {
                      let bname = ResourceExt::name(&bridge);
                      let do_not_delete = env_var!(optional "DO_NOT_DELETE_OBJECTS", default:"FALSE");
                      if do_not_delete == "TRUE" {
                        warn!("delete event for {} (not executed because of DO_NOT_DELETE_OBJECTS setting)",bname);
                      }else{
                        delete_bridge(&bridge);
                      }
                      let mut res = KNOWN_BRIDGES.lock().unwrap();
                      res.remove(&bname);
                      last_version = ResourceExt::resource_version(&bridge).unwrap();                   
                    },
                    WatchEvent::Error(e) => {
                      if e.code == 410 && e.reason=="Expired" {
                        //fail silently
                        trace!("resource_version too old, resetting offset to 0");
                        last_version="0".to_owned();
                      }else{
                        error!("Error {:?}", e);
                        error!("resetting offset to 0");
                        last_version="0".to_owned();
                      }
                    },
                    _ => {}
                  };
                },
                None => {
                  debug!("request loop returned empty");  
                  break;
                }
              }
            },
            Err(err) => {
              debug!("error on request loop {:?}",err);  
              break;
            }
          }
        }
      },
      Err(_err) => {
        //ignore connection reset
      }
    }
  }
}

async fn get_bridge_client() -> Api<Bridge>{
  let client = Client::try_default().await.expect("getting default client");
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  let crds: Api<Bridge> = Api::namespaced(client, &namespace);
  return crds;
}

fn create_bridge(bridge: &Bridge){
  let bname = bridge.metadata.name.clone().unwrap();
  info!("creating bridge {}",bname);
  let session = ADMIN_CONNECTION.lock().unwrap();
  let mut bridge_info = BridgeInfo{
    source: Destination::Topic(bridge.spec.source_name.clone()),
    target: Destination::Queue(bridge.spec.target_name.clone()),
    selector: None,
  };
  let mut source_type = bridge.spec.source_type.clone();
  source_type.make_ascii_uppercase();
  if source_type.starts_with("QUEUE") {
    bridge_info.source = Destination::Queue(bridge.spec.source_name.clone())
  }
  if source_type.starts_with("TOPIC") {
    bridge_info.source = Destination::Topic(bridge.spec.source_name.clone());
  }
  let mut target_type = bridge.spec.target_type.clone();
  target_type.make_ascii_uppercase();
  if target_type.starts_with("QUEUE") {
    bridge_info.target = Destination::Queue(bridge.spec.target_name.clone());
  }
  if target_type.starts_with("TOPIC") {
    bridge_info.target = Destination::Topic(bridge.spec.target_name.clone());
  }
  match bridge.spec.selector.clone() {
    Some(sel) => {
      bridge_info.selector = Some(sel);
    },
    None => {},
  }
  tibco_ems::admin::create_bridge(&session, &bridge_info);
}

fn delete_bridge(bridge: &Bridge){
  let bname = bridge.metadata.name.clone().unwrap();
  info!("deleting bridge {}",bname);
  let session = ADMIN_CONNECTION.lock().unwrap();
  let mut bridge_info = BridgeInfo{
    source: Destination::Topic(bridge.spec.source_name.clone()),
    target: Destination::Queue(bridge.spec.target_name.clone()),
    selector: None,
  };
  let mut source_type = bridge.spec.source_type.clone();
  source_type.make_ascii_uppercase();
  if source_type.starts_with("QUEUE") {
    bridge_info.source = Destination::Queue(bridge.spec.source_name.clone());
  }
  if source_type.starts_with("TOPIC") {
    bridge_info.source = Destination::Topic(bridge.spec.source_name.clone());
  }
  let mut target_type = bridge.spec.target_type.clone();
  target_type.make_ascii_uppercase();
  if target_type.starts_with("QUEUE") {
    bridge_info.target = Destination::Queue(bridge.spec.target_name.clone());
  }
  if target_type.starts_with("TOPIC") {
    bridge_info.target = Destination::Topic(bridge.spec.target_name.clone());
  }
  tibco_ems::admin::delete_bridge(&session, &bridge_info);
}