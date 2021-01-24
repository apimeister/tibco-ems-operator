use kube::{api::{Api, ListParams, Meta, PostParams}, Client};
use kube::api::WatchEvent;
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
use std::ffi::CString;
use env_var::env_var;

use crate::ems;

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


pub static KNOWN_QUEUES: Lazy<Mutex<HashMap<String, Queue>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

pub static QUEUES: Lazy<Mutex<HashMap<String,ems::QueueInfo>>> = Lazy::new(|| 
  Mutex::new(HashMap::new() ) );
  
pub async fn watch_queues() -> Result<()>{
  let crds: Api<Queue> = get_queue_client().await;
  let updater: Api<Queue> = crds.clone(); 
  let lp = ListParams::default();

  let mut last_version: String = "0".to_string();
  info!("subscribing events of type queues.tibcoems.apimeister.com/v1");
  loop{
    debug!("Q: new loop iteration with offset {}",last_version);
    let mut stream = crds.watch(&lp, &last_version).await.unwrap().boxed();
    loop {
      debug!("Q: new stream item");
      let stream_result = stream.try_next().await;
      match stream_result {
        Ok(status_obj) => {
          match status_obj {
            Some(status) => {
              match status {
                WatchEvent::Added(mut queue) =>{
                  let qname = get_queue_name(&queue);
                  let name = Meta::name(&queue);
                  {
                    let mut res = KNOWN_QUEUES.lock().unwrap();
                    match res.get(&qname) {
                      Some(_queue) => info!("queue already known {}", &qname),
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
                },
                WatchEvent::Deleted(queue) =>{
                  delete_queue(queue.clone());
                  let mut res = KNOWN_QUEUES.lock().unwrap();
                  let qname = get_queue_name(&queue);
                  res.remove(&qname);           
                },
                WatchEvent::Error(e) => {
                  error!("Error {}", e);
                  error!("resetting offset to 0");
                  last_version="0".to_owned();
                },
                _ => {},
              };
            },
            None => {
              debug!("Q: request loop returned empty");  
              break;
            }
          }
        },
        Err(err) => {
          debug!("Q: error on request loop {:?}",err);  
          break;
        }
      }
    }
  }
}

pub async fn watch_queues_status() -> Result<()>{
  let status_refresh_in_ms: u64 = env_var!(optional "STATUS_REFRESH_IN_MS", default: "10000").parse().unwrap();
  let mut interval = time::interval(Duration::from_millis(status_refresh_in_ms));
  loop {
    let result = ems::get_queue_stats();
    for qinfo in &result {
      //update prometheus
      {
        let mut c_map = QUEUES.lock().unwrap();
        c_map.insert(qinfo.queue_name.clone(),qinfo.clone());
      }
      //update k8s state
      let mut q : Option<Queue> = None;
      {
        let mut res = KNOWN_QUEUES.lock().unwrap();
        match res.get(&qinfo.queue_name) {
          Some(queue) =>{
            let mut local_q = queue.clone();
            match &queue.status {
              Some(status) => {
                if status.pendingMessages != qinfo.pending_messages as i64
                  || status.consumerCount != qinfo.consumers as i32 {
                  local_q.status = Some(QueueStatus{
                      pendingMessages: qinfo.pending_messages,
                      consumerCount: qinfo.consumers as i32});
                  q = Some(local_q.clone());  
                  res.insert(qinfo.queue_name.to_owned(),local_q);
                }
              },
              None => {
                local_q.status = Some(QueueStatus{
                  pendingMessages: qinfo.pending_messages,
                  consumerCount: qinfo.consumers as i32});
                q = Some(local_q.clone());  
                res.insert(qinfo.queue_name.to_owned(),local_q);
              },
            }
          },
          None => {},
        }
      }
      match q {
        Some(mut local_q) => {
          let obj_name = get_obj_name_from_queue(&local_q);
          info!("updating queue status for {}",obj_name);
          let updater: Api<Queue> = get_queue_client().await;
          let latest_queue: Queue = updater.get(&obj_name).await.unwrap();
          local_q.metadata.resource_version=Meta::resource_ver(&latest_queue);
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
    interval.tick().await;
  }
}

async fn get_queue_client() -> Api<Queue>{
  let config = Config::infer().await.unwrap();
  let client: kube::Client = Client::new(config);
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
  let admin_queue_name = "$sys.admin";
  let create_destination = 18;
  //create queue map-message
  let mut msg: usize = 0;
  unsafe {
    let status = tibco_ems_sys::tibemsMapMsg_Create(&mut msg);
    println!("tibemsMapMsg_Create {:?}",status);
    let c_dn = CString::new("dn".to_string()).unwrap();
    let destination_name = get_queue_name(queue);
    let c_destination_name = CString::new(destination_name).unwrap();
    tibco_ems_sys::tibemsMapMsg_SetString(msg, c_dn.as_ptr(), c_destination_name.as_ptr());
    println!("tibemsMapMsg_SetString {:?}",status);
    let c_dt = CString::new("dt".to_string()).unwrap();
    tibco_ems_sys::tibemsMapMsg_SetInt(msg, c_dt.as_ptr(), 1);
    println!("tibemsMapMsg_SetInt {:?}",status);
    match queue.spec.maxbytes {
      Some(val) => {
        let c_mb = CString::new("mb").unwrap();
        tibco_ems_sys::tibemsMapMsg_SetLong(msg, c_mb.as_ptr(), val);
        println!("tibemsMapMsg_SetInt {:?}",status);
      },
      _ => {},
    }
    match queue.spec.maxmsgs {
      Some(val) => {
        let c_mm = CString::new("mm").unwrap();
        tibco_ems_sys::tibemsMapMsg_SetLong(msg, c_mm.as_ptr(), val);
        println!("tibemsMapMsg_SetInt {:?}",status);
      },
      _ => {},
    }
    match queue.spec.global {
      Some(val) => {
        let c_global = CString::new("global").unwrap();
        let mut global_flag = tibco_ems_sys::tibems_bool::TIBEMS_FALSE;
        if val {
          global_flag = tibco_ems_sys::tibems_bool::TIBEMS_TRUE;
        }
        tibco_ems_sys::tibemsMapMsg_SetBoolean(msg, c_global.as_ptr(),  global_flag);
        println!("tibemsMapMsg_SetInt {:?}",status);
      },
      _ => {},
    }

    //header
    let c_msg_ext = CString::new("JMS_TIBCO_MSG_EXT").unwrap();
    tibco_ems_sys::tibemsMsg_SetBooleanProperty(msg, c_msg_ext.as_ptr(), tibco_ems_sys::tibems_bool::TIBEMS_TRUE);  
    let c_code = CString::new("code").unwrap();
    let status = tibco_ems_sys::tibemsMsg_SetIntProperty(msg, c_code.as_ptr(), create_destination);
    println!("tibemsMsg_SetIntProperty {:?}",status);
    let c_save = CString::new("save").unwrap();
    tibco_ems_sys::tibemsMsg_SetBooleanProperty(msg, c_save.as_ptr(), tibco_ems_sys::tibems_bool::TIBEMS_TRUE);
    let c_arseq = CString::new("arseq").unwrap();
    tibco_ems_sys::tibemsMsg_SetIntProperty(msg, c_arseq.as_ptr(), 1);

    //sending
    let mut session_pointer: usize = 0;
    let admin = ems::ADMIN_CONNECTION.lock().unwrap();
    let status = tibco_ems_sys::tibemsConnection_CreateSession(admin.pointer, &mut session_pointer, tibco_ems_sys::tibems_bool::TIBEMS_FALSE, tibco_ems_sys::tibemsAcknowledgeMode::TIBEMS_AUTO_ACKNOWLEDGE);
    println!("tibemsConnection_CreateSession {:?}",status);
    let mut dest:usize = 0;
    let c_destination = CString::new(admin_queue_name).unwrap();
    let status = tibco_ems_sys::tibemsDestination_Create(&mut dest, tibco_ems_sys::tibemsDestinationType::TIBEMS_QUEUE, c_destination.as_ptr());
    println!("tibemsDestination_Create {:?}",status);
    let mut producer: usize = 0;
    let status = tibco_ems_sys::tibemsSession_CreateProducer(session_pointer,&mut producer,dest);
    println!("tibemsSession_CreateProducer {:?}",status);
    let status = tibco_ems_sys::tibemsMsgProducer_Send(producer, msg);
    println!("tibemsMsgProducer_Send {:?}",status);

    //closing resources
    let status = tibco_ems_sys::tibemsMsg_Destroy(msg);
    println!("tibemsMsg_Destroy {:?}",status);
    //destroy producer
    let status = tibco_ems_sys::tibemsMsgProducer_Close(producer);
    println!("tibemsMsgProducer_Close {:?}",status);
    //destroy destination
    let status = tibco_ems_sys::tibemsDestination_Destroy(dest);
    println!("tibemsDestination_Destroy {:?}",status);
    let status = tibco_ems_sys::tibemsSession_Close(admin.pointer);
    println!("tibemsSession_Close {:?}",status);
  }
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

fn delete_queue(queue: Queue){
  let queue_name = get_queue_name(&queue);
  info!("deleting queue {}", queue_name);
  let script = "delete queue ".to_owned()+&queue_name+"\n";
  let _result = ems::run_tibems_script(script);
}