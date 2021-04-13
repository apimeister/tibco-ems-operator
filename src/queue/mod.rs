use kube::{api::{Api, ListParams, Resource, PostParams}, Client};
use kube::api::WatchEvent;
use kube::Service;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;
use tokio::time::{self, Duration};
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use hyper::Result;
use schemars::JsonSchema;
use env_var::env_var;
use core::convert::TryFrom;
use tibco_ems::Session;
use tibco_ems::admin::QueueInfo;
use super::scaler::ScaleUp;
use super::scaler::ScaleDown;
use super::scaler::get_epoch_seconds;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug, JsonSchema)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", 
    kind="Queue",
    status="QueueStatus",
    namespaced)]
#[allow(non_snake_case)]
pub struct QueueSpec {
  pub name: Option<String>,
  pub expiration: Option<u32>,
  pub global: Option<bool>,
  pub maxbytes: Option<i64>,
  pub maxmsgs: Option<i64>,
  pub maxRedelivery: Option<u32>,
  pub overflowPolicy: Option<u8>,
  pub prefetch: Option<u32>,
  pub redeliveryDelay: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[allow(non_snake_case)]
pub struct QueueStatus {
  pub pendingMessages: i64,
  pub consumerCount: i32,
}

/// registered queues within kubernetes
pub static KNOWN_QUEUES: Lazy<Mutex<HashMap<String, Queue>>> = Lazy::new(|| Mutex::new(HashMap::new()) );
/// all queues present on the EMS
pub static QUEUES: Lazy<Mutex<HashMap<String,QueueInfo>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

///used for retrieving queue statistics
static QUEUE_ADMIN_CONNECTION: Lazy<Mutex<Session>> = Lazy::new(|| Mutex::new(super::init_admin_connection()));
///used for sending admin operations
static ADMIN_CONNECTION: Lazy<Mutex<Session>> = Lazy::new(|| Mutex::new(super::init_admin_connection()));
  
pub async fn watch_queues() -> Result<()>{
  let crds: Api<Queue> = get_queue_client().await;
  let updater: Api<Queue> = crds.clone(); 
  let lp = ListParams::default();

  let mut last_version: String = "0".to_string();
  info!("subscribing events of type queues.tibcoems.apimeister.com/v1");
  loop{
    debug!("new loop iteration with offset {}",last_version);
    let watch_result = crds.watch(&lp, &last_version).await;
    match watch_result {
      Ok(str_result) => {
        let mut stream = str_result.boxed();
        loop {
          let stream_result = stream.try_next().await;
          match stream_result {
            Ok(status_obj) => {
              match status_obj {
                Some(status) => {
                  debug!("new stream item");
                  match status {
                    WatchEvent::Added(mut queue) =>{
                      let qname = get_queue_name(&queue);
                      let name = Resource::name(&queue);
                      {
                        let mut res = KNOWN_QUEUES.lock().unwrap();
                        match res.get(&qname) {
                          Some(_queue) => debug!("queue already known {}", &qname),
                          None => {
                            info!("adding queue {}", &qname);
                            create_queue(&mut queue);
                            let q = (&queue).clone();
                            let n = (&qname).to_string();
                            res.insert(n, q);
                          },
                        }
                      }
                      match queue.status {
                        None =>{
                          let q_json = serde_json::to_string(&queue).unwrap();
                          let pp = PostParams::default();
                          let _result = updater.replace_status(&name, &pp, q_json.as_bytes().to_vec()).await;
                        },
                        _ => {},
                      };
                      last_version = Resource::resource_ver(&queue).unwrap();
                    },
                    WatchEvent::Deleted(queue) =>{
                      let do_not_delete = env_var!(optional "DO_NOT_DELETE_OBJECTS", default:"FALSE");
                      let qname = get_queue_name(&queue);
                      if do_not_delete == "TRUE" {
                        warn!("delete queue {} (not executed because of DO_NOT_DELETE_OBJECTS setting)",qname);
                      }else{
                        delete_queue(&queue);
                      }
                      let mut res = KNOWN_QUEUES.lock().unwrap();
                      res.remove(&qname);
                      last_version = Resource::resource_ver(&queue).unwrap();
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
                    _ => {},
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

pub async fn watch_queues_status() -> Result<()>{
  let status_refresh_in_ms: u64 = env_var!(optional "STATUS_REFRESH_IN_MS", default: "10000").parse().unwrap();
  let read_only = env_var!(optional "READ_ONLY", default:"FALSE");
  let mut interval = time::interval(Duration::from_millis(status_refresh_in_ms));
  
  loop {
    let result;
    {
      let session = QUEUE_ADMIN_CONNECTION.lock().unwrap();
      result = tibco_ems::admin::list_all_queues(&session);
    }
    for qinfo in &result {
      let queue_name = qinfo.name.clone();
      //update prometheus
      {
        let mut c_map = QUEUES.lock().unwrap();
        c_map.insert(qinfo.name.clone(),qinfo.clone());
      }
      //update scaler
      let scaling = env_var!(optional "ENABLE_SCALING", default:"FALSE");
      if scaling == "TRUE" {
        let targets = super::scaler::SCALE_TARGETS.lock().unwrap();
        if targets.contains_key(&queue_name) {
          let deployment_name = targets.get(&queue_name).unwrap();
          let mut scalings = super::scaler::KNOWN_SCALINGS.lock().unwrap();
          let scaling = scalings.get(deployment_name).unwrap();
          if qinfo.pending_messages.unwrap() > 0 {
            //scale up
            let s = scaling.clone();
            let s2 = s.on_scale_up(ScaleUp{
              activity_timestamp: get_epoch_seconds()
            });
            scalings.insert(deployment_name.clone(),s2);
          }else{
            //scale down
            let s = scaling.clone();
            let s2 = s.on_scale_down(ScaleDown{
              activity_timestamp: get_epoch_seconds()
            });
            scalings.insert(deployment_name.clone(),s2);
          }
        }
      }    
      //update k8s state
      if read_only == "FALSE" {
        let mut q : Option<Queue> = None;
        {
          let mut res = KNOWN_QUEUES.lock().unwrap();
          match res.get(&qinfo.name) {
            Some(queue) =>{
              let mut local_q = queue.clone();
              match &queue.status {
                Some(status) => {
                  let mut consumer_count = 0;
                  match qinfo.consumer_count {
                    Some(val) => {
                      consumer_count = val;
                    },
                    None =>{},
                  }
                  let mut pending_messages = 0;
                  match qinfo.pending_messages {
                    Some(val) => {
                      pending_messages = val;
                    },
                    None =>{},
                  }
                  if status.pendingMessages != pending_messages 
                    || status.consumerCount != consumer_count {
                    local_q.status = Some(QueueStatus{
                        pendingMessages: pending_messages,
                        consumerCount: consumer_count});
                    q = Some(local_q.clone());  
                    res.insert(qinfo.name.to_owned(),local_q);
                  }
                },
                None => {
                  local_q.status = Some(QueueStatus{
                    pendingMessages: qinfo.pending_messages.unwrap(),
                    consumerCount: qinfo.consumer_count.unwrap()});
                  q = Some(local_q.clone());  
                  res.insert(qinfo.name.to_owned(),local_q);
                },
              }
            },
            None => {
              //in read_only mode, watch all queues
              let read_only = env_var!(optional "READ_ONLY", default:"FALSE");
              if read_only == "TRUE" {
                
              }
            },
          }
        }
        match q {
          Some(mut local_q) => {
            let obj_name = get_obj_name_from_queue(&local_q);
            debug!("updating queue status for {}",obj_name);
            let updater: Api<Queue> = get_queue_client().await;
            let latest_queue: Queue = updater.get(&obj_name).await.unwrap();
            local_q.metadata.resource_version=Resource::resource_ver(&latest_queue);
            let q_json = serde_json::to_string(&local_q).unwrap();
            let pp = PostParams::default();
            let result = updater.replace_status(&obj_name, &pp, q_json.as_bytes().to_vec()).await;
            match result {
              Ok(_ignore) => {},
              Err(err) => {
                error!("error while updating queue object");
                error!("{:?}",err);
              },
            }
          },
          None => {},
        }
      }
    }
    interval.tick().await;
  }
}

async fn get_queue_client() -> Api<Queue>{
  let config = Config::infer().await.unwrap();
  let service = Service::try_from(config).unwrap();
  let client: kube::Client = Client::new(service);
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  let crds: Api<Queue> = Api::namespaced(client, &namespace);
  return crds;
}
fn get_queue_name(queue: &Queue) -> String {
  let mut qname: String = String::from("");
  //check for name in spec
  match &queue.spec.name {
    Some(q) => qname=q.to_owned(),
    None =>{
      match &queue.metadata.name {
        Some(n) =>{
          qname = n.to_owned();
          qname.make_ascii_uppercase();
        },
        _ => {},
      }
    },
  }
  return qname;
}
fn get_obj_name_from_queue(queue: &Queue) -> String {
  return queue.metadata.name.clone().unwrap();
}

fn create_queue(queue: &mut Queue){
  let qname = get_queue_name(queue);

  let queue_info = QueueInfo{
    name: qname,
    max_bytes: queue.spec.maxbytes,
    max_messages: queue.spec.maxmsgs,
    global: queue.spec.global,
    ..Default::default()
  };
  let session = ADMIN_CONNECTION.lock().unwrap();
  tibco_ems::admin::create_queue(&session, &queue_info);

  //propagate defaults
  match queue.spec.maxmsgs {
    None => queue.spec.maxmsgs=Some(0),
    _ => {},
  }
  match queue.spec.expiration {
    None => queue.spec.expiration=Some(0),
    _ => {},
  }
  queue.spec.overflowPolicy=Some(0);
  queue.spec.prefetch=Some(0);
  queue.spec.global=Some(false);
  queue.spec.maxbytes=Some(0);
  queue.spec.redeliveryDelay=Some(0);
  queue.spec.maxRedelivery=Some(0);
  let status = QueueStatus{pendingMessages: 0,consumerCount: 0};
  queue.status =  Some(status);
}

fn delete_queue(queue: &Queue){
  let qname = get_queue_name(queue);
  info!("deleting queue {}", qname);
  let session = ADMIN_CONNECTION.lock().unwrap();
  tibco_ems::admin::delete_queue(&session, &qname);
}