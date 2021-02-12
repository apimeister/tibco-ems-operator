use core::convert::TryFrom;
use env_var::env_var;
use futures::{StreamExt, TryStreamExt};
use kube::{api::{Api, ListParams, Meta}, Client};
use kube::api::WatchEvent;
use kube::Service;
use kube_derive::CustomResource;
use kube::config::Config;
use hyper::Result;
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tibco_ems::MapMessage;
use tibco_ems::TypedValue;
use tibco_ems::Destination;
use tibco_ems::DestinationType;
use std::sync::Mutex;
use once_cell::sync::Lazy;

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

pub static KNOWN_BRIDGES: Lazy<Mutex<HashMap<String, Bridge>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

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
                  let bname = Meta::name(&bridge);
                  {
                    let mut res = KNOWN_BRIDGES.lock().unwrap();
                    match res.get(&bname) {
                      Some(_bridge) => debug!("bridge already known {}", &bname),
                      None => {
                        info!("Added {}@{}", bname, &ver);
                        create_bridge(&bridge);
                        last_version = (ver.parse::<i64>().unwrap() + 1).to_string(); 
                        let b = bridge.clone();
                        let n = bname.clone();
                        res.insert(n, b);
                      },
                    }
                  }
                },
                WatchEvent::Modified(bridge) => {
                  let ver = Meta::resource_ver(&bridge).unwrap();
                  info!("Modified {}@{}", Meta::name(&bridge), &ver);
                  create_bridge(&bridge);
                  last_version = (ver.parse::<i64>().unwrap() + 1).to_string();            
                }
                WatchEvent::Deleted(bridge) => {
                  let ver = Meta::resource_ver(&bridge).unwrap();
                  let bname = Meta::name(&bridge);
                  let do_not_delete = env_var!(optional "DO_NOT_DELETE_OBJECTS", default:"FALSE");
                  if do_not_delete == "TRUE" {
                    warn!("delete event for {} (not executed because of DO_NOT_DELETE_OBJECTS setting)",bname);
                  }else{
                    delete_bridge(&bridge);
                  }
                  last_version = (ver.parse::<i64>().unwrap() + 1).to_string();   
                  let mut res = KNOWN_BRIDGES.lock().unwrap();
                  res.remove(&bname);                    
                },
                WatchEvent::Error(e) => {
                  if e.code == 410 && e.reason == "Expired" {
                    last_version="0".to_owned();
                  }else{
                    error!("Error: {}", e);
                    last_version="0".to_owned();
                  }
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
  let service = Service::try_from(config).unwrap();
  let client: kube::Client = Client::new(service);
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  let crds: Api<Bridge> = Api::namespaced(client, &namespace);
  return crds;
}

fn create_bridge(bridge: &Bridge){
  let command_create_bridge = 220;
  let admin_queue_name = "$sys.admin";
  let bname = bridge.metadata.name.clone().unwrap();
  info!("creating bridge {}",bname);
  //create bridge map-message
  let source_name = bridge.spec.source_name.clone();
  let mut source_type = bridge.spec.source_type.clone();
  let target_name = bridge.spec.target_name.clone();
  let mut target_type = bridge.spec.target_type.clone();
  let mut msg: MapMessage = Default::default();
  msg.body.insert("sn".to_string(), TypedValue::string_value(source_name));
  source_type.make_ascii_uppercase();
  if source_type.starts_with("QUEUE") {
    msg.body.insert("st".to_string(), TypedValue::int_value(1));
  }
  if source_type.starts_with("TOPIC") {
    msg.body.insert("st".to_string(), TypedValue::int_value(2));
  }
  msg.body.insert("tn".to_string(), TypedValue::string_value(target_name));
  target_type.make_ascii_uppercase();
  if target_type.starts_with("QUEUE") {
    msg.body.insert("tt".to_string(), TypedValue::int_value(1));
  }
  if target_type.starts_with("TOPIC") {
    msg.body.insert("tt".to_string(), TypedValue::int_value(2));
  }
  match bridge.spec.selector.clone() {
    Some(sel) => {
      msg.body.insert("sel".to_string(), TypedValue::string_value(sel));
    },
    None => {},
  }
  //header
  let mut header: HashMap<String,TypedValue> = HashMap::new();
  //actual boolean
  header.insert("JMS_TIBCO_MSG_EXT".to_string(),TypedValue::bool_value(true));
  header.insert("code".to_string(),TypedValue::int_value(command_create_bridge));
  header.insert("save".to_string(),TypedValue::bool_value(true));
  header.insert("arseq".to_string(),TypedValue::int_value(1));

  msg.header = Some(header);

  let admin_session = ems::ADMIN_CONNECTION.lock().unwrap();

  let dest = Destination{
    destination_type: DestinationType::Queue,
    destination_name: admin_queue_name.to_string(),
  };
  let result = admin_session.send_message(dest, msg.into());
  match result {
    Ok(_) => {},
    Err(err) => {
      error!("error while creating bridge {}: {}",bname,err);
    },
  }
}

fn delete_bridge(bridge: &Bridge){
  let command_delete_bridge = 221;
  let admin_queue_name = "$sys.admin";
  let bname = bridge.metadata.name.clone().unwrap();
  info!("deleting bridge {}",bname);
  let source_name = bridge.spec.source_name.clone();
  let mut source_type = bridge.spec.source_type.clone();
  let target_name = bridge.spec.target_name.clone();
  let mut target_type = bridge.spec.target_type.clone();

  //create bridge map-message
  let mut msg: MapMessage = Default::default();
  msg.body.insert("sn".to_string(), TypedValue::string_value(source_name));
  source_type.make_ascii_uppercase();
  if source_type.starts_with("QUEUE") {
    msg.body.insert("st".to_string(), TypedValue::int_value(1));
  }
  if source_type.starts_with("TOPIC") {
    msg.body.insert("st".to_string(), TypedValue::int_value(2));
  }
  msg.body.insert("tn".to_string(), TypedValue::string_value(target_name));
  target_type.make_ascii_uppercase();
  if target_type.starts_with("QUEUE") {
    msg.body.insert("tt".to_string(), TypedValue::int_value(1));
  }
  if target_type.starts_with("TOPIC") {
    msg.body.insert("tt".to_string(), TypedValue::int_value(2));
  }
  //header
  let mut header: HashMap<String,TypedValue> = HashMap::new();
  //actual boolean
  header.insert("JMS_TIBCO_MSG_EXT".to_string(),TypedValue::bool_value(true));
  header.insert("code".to_string(),TypedValue::int_value(command_delete_bridge));
  header.insert("save".to_string(),TypedValue::bool_value(true));
  header.insert("arseq".to_string(),TypedValue::int_value(1));
  msg.header = Some(header);

  let admin_session = ems::ADMIN_CONNECTION.lock().unwrap();

  let dest = Destination{
    destination_type: DestinationType::Queue,
    destination_name: admin_queue_name.to_string(),
  };
  let result = admin_session.send_message(dest, msg.into());
  match result {
    Ok(_) => {},
    Err(err) => {
      error!("error while deleting bridge {}: {}",bname,err);
    }
  }
  
}

/*
create bridge
tt = 1
st = 1
tn = q.test.2
sn Q.TEST.1
header:
code 220 => create
code 221 => delete
save: true
*/
