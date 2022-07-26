use env_var::env_var;
use futures::{StreamExt, TryStreamExt};
use kube::{api::{Api, ListParams, ResourceExt, WatchEvent}, Client};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tibco_ems::admin::BridgeInfo;
use tibco_ems::{Session, Destination};
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

pub async fn watch_bridges() -> Result<(),()>{
  let crds: Api<Bridge> = get_bridge_client().await;
  let mut lp = ListParams::default();

  let responsible_for = super::RESPONSIBLE_FOR.lock().unwrap().clone();
  if !responsible_for.is_empty() {
    info!("subscribing to events of type bridges.tibcoems.apimeister.com/v1 for instance {responsible_for}");
    lp = lp.labels(format!("tibcoems.apimeister.com/owner={responsible_for}").as_str());
  } else {
    lp = lp.labels("!tibcoems.apimeister.com/owner");
    info!("subscribing to events of type bridges.tibcoems.apimeister.com/v1");
  }

  let mut last_version = String::from("0");
  loop{
    debug!("new loop iteration with offset {}",last_version);
    let watch_result = crds.watch(&lp, &last_version).await;
    let str_result = match watch_result { Ok(x) => x, Err(_) => continue };
    let mut stream = str_result.boxed();
    loop {
      let stream_result = stream.try_next().await;
      let status_obj = match stream_result { Ok(x) => x, Err(e) => { debug!("error on request loop {:?}", e); break } };
      let status = match status_obj { Some(x) => x, None => { debug!("request loop returned empty"); break } };
      debug!("new stream item");

      match status {
        WatchEvent::Added(bridge) =>{
          let mut res = KNOWN_BRIDGES.lock().unwrap();
          let bridge_name = ResourceExt::name(&bridge);
            match res.get(&bridge_name) {
              Some(_bridge) => debug!("bridge already known {}", &bridge_name),
              None => {
                info!("adding bridge {}", &bridge_name);
                create_bridge(&bridge);
                res.insert(bridge_name, bridge.clone());
              },
            }
          last_version = ResourceExt::resource_version(&bridge).unwrap();
        },
        WatchEvent::Modified(bridge) => {
          let bridge_name = ResourceExt::name(&bridge);
          info!("Modified {}", bridge_name);
          create_bridge(&bridge);
          last_version = ResourceExt::resource_version(&bridge).unwrap();          
        },
        WatchEvent::Deleted(bridge) => {
          let bridge_name = ResourceExt::name(&bridge);
          let do_not_delete = env_var!(optional "DO_NOT_DELETE_OBJECTS", default:"FALSE");
          if do_not_delete == "TRUE" {
            warn!("delete event for {} (not executed because of DO_NOT_DELETE_OBJECTS setting)", bridge_name);
          } else {
            info!("deleting bridge {}", &bridge_name);
            delete_bridge(&bridge);
          }
          let mut res = KNOWN_BRIDGES.lock().unwrap();
          res.remove(&bridge_name);
          last_version = ResourceExt::resource_version(&bridge).unwrap();
        },
        WatchEvent::Error(e) => {
          if e.code == 410 && e.reason=="Expired" {
            //fail silently
            trace!("resource_version too old, resetting offset to 0");
          }else{
            error!("Error {:?}", e);
            error!("resetting offset to 0");
          }
          last_version="0".to_owned();
        },
        _ => {},
      };
    }
  }
}

async fn get_bridge_client() -> Api<Bridge>{
  let client = Client::try_default().await.expect("getting default client");
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  Api::namespaced(client, &namespace)
}

fn create_bridge(bridge: &Bridge){
  let bridge_object = create_bridge_object(bridge);
  // create bridge on server
  let session = ADMIN_CONNECTION.lock().unwrap();
  let result = tibco_ems::admin::create_bridge(&session, &bridge_object);
  match result {
    Ok(_) => debug!("bridge created successfully"),
    Err(err) => {
      error!("failed to create bridge: {:?}",err);
      panic!("failed to create bridge");
    },
  }
}

fn delete_bridge(bridge: &Bridge){
  let bridge_object = create_bridge_object(bridge);
  let session = ADMIN_CONNECTION.lock().unwrap();
  let result = tibco_ems::admin::delete_bridge(&session, &bridge_object);
  match result {
    Ok(_) => debug!("bridge deleted"),
    Err(err) => {
      error!("failed to delete bridge: {:?}",err);
      panic!("failed to delete bridge");
    },
  }
}

fn create_bridge_object(bridge: &Bridge) -> BridgeInfo {
  // generate default bridge -> T:Q
  let mut bridge_info = BridgeInfo{
    source: Destination::Topic(bridge.spec.source_name.clone()),
    target: Destination::Queue(bridge.spec.target_name.clone()),
    selector: None,
  };
  // if source is not a topic -> Q:Q
  let mut source_type = bridge.spec.source_type.clone();
  source_type.make_ascii_uppercase();
  if source_type.starts_with('Q') { bridge_info.source = Destination::Queue(bridge.spec.source_name.clone()); }
  // if target is not a queue -> T:T or Q:T depending if above set to Q
  let mut target_type = bridge.spec.target_type.clone();
  target_type.make_ascii_uppercase();
  if target_type.starts_with('T') { bridge_info.target = Destination::Topic(bridge.spec.target_name.clone()); }
  // add selector if given
  match &bridge.spec.selector {
    Some(sel) => bridge_info.selector = Some(sel.clone()),
    None => {},
  }

  // show what we have created in debug mode
  debug!("{:?}", bridge_info);
  bridge_info
}
