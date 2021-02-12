use kube::{api::{Api, ListParams, Meta, PostParams}, Client};
use kube::api::WatchEvent;
use kube::Service;
use core::convert::TryFrom;
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

use crate::ems;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug, JsonSchema)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", 
    kind="Topic",
    status="TopicStatus",
    namespaced)]
#[allow(non_snake_case)]
pub struct TopicSpec {
  pub name: Option<String>,
  pub expiration: Option<u32>,
  pub global: Option<bool>,
  pub maxbytes: Option<u32>,
  pub maxmsgs: Option<u32>,
  pub overflowPolicy: Option<u8>,
  pub prefetch: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[allow(non_snake_case)]
pub struct TopicStatus {
  pub pendingMessages: i64,
  pub subscribers: i32,
  pub durables: i32,
}

pub static KNOWN_TOPICS: Lazy<Mutex<HashMap<String, Topic>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

pub static TOPICS: Lazy<Mutex<HashMap<String,ems::TopicInfo>>> = Lazy::new(|| 
  Mutex::new(HashMap::new() ) );

pub async fn watch_topics() -> Result<()>{
  let crds: Api<Topic> = get_topic_client().await;
  let updater: Api<Topic> = crds.clone(); 
  let lp = ListParams::default();

  let mut last_version: String = "0".to_owned();
  info!("subscribing events of type topics.tibcoems.apimeister.com/v1");
  loop{
    debug!("T: new loop iteration with offset {}",last_version);
    let mut stream = crds.watch(&lp, &last_version).await.unwrap().boxed();
    loop {
      debug!("T: new stream item");
      let stream_result = stream.try_next().await;
      match stream_result {
        Ok(status_obj) => {
          match status_obj {
            Some(status) => {
              match status {
                WatchEvent::Added(mut topic) =>{
                  let topic_name = get_topic_name(&topic);
                  let name = Meta::name(&topic);
                  {
                    let mut res = KNOWN_TOPICS.lock().unwrap();
                    match res.get(&topic_name) {
                      Some(_queue) => debug!("topic already known {}", &topic_name),
                      None => {
                        info!("adding topic {}", &topic_name);
                        create_topic(&mut topic);
                        let q = (&topic).clone();
                        let n = (&topic_name).clone();
                        res.insert(n, q);
                      },
                    }
                  }
                  match topic.status {
                    None =>{
                      let q_json = serde_json::to_string(&topic).unwrap();
                      let pp = PostParams::default();
                      let _result = updater.replace_status(&name, &pp, q_json.as_bytes().to_vec()).await;
                    },
                    _ => {},
                  };
                },
                WatchEvent::Deleted(topic) =>{
                  let do_not_delete = env_var!(optional "DO_NOT_DELETE_OBJECTS", default:"FALSE");
                  let tname = get_topic_name(&topic);
                  if do_not_delete == "TRUE" {
                    warn!("delete event for {} (not executed because of DO_NOT_DELETE_OBJECTS setting)",tname);
                  }else{
                    delete_topic(topic);
                  }                  
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
              debug!("T: request loop returned empty");  
              break;
            }
          }
        },
        Err(err) => {
          debug!("T: error on request loop {:?}",err);  
          break;
        }
      }
    }
  }
}

pub async fn watch_topics_status() -> Result<()>{
  let status_refresh_in_ms: u64 = env_var!(optional "STATUS_REFRESH_IN_MS", default: "10000").parse().unwrap();
  let mut interval = time::interval(Duration::from_millis(status_refresh_in_ms));
  loop {
    let result = ems::get_topic_stats();
    for tinfo in &result {
      //update prometheus
      {
        let mut c_map = TOPICS.lock().unwrap();
        c_map.insert(tinfo.topic_name.clone(),tinfo.clone());
      }
      //update k8s state
      let mut t : Option<Topic> = None;
      {
        let mut res = KNOWN_TOPICS.lock().unwrap();
        match res.get(&tinfo.topic_name) {
          Some(topic) =>{
            let mut local_t = topic.clone();
            match &topic.status {
              Some(status) => {
                if status.pendingMessages != tinfo.pending_messages
                  || status.subscribers != tinfo.subscribers
                  || status.durables != tinfo.durables {
                  local_t.status = Some(TopicStatus{
                      pendingMessages: tinfo.pending_messages,
                      subscribers: tinfo.subscribers,
                      durables: tinfo.durables });
                  t = Some(local_t.clone());  
                  res.insert(tinfo.topic_name.to_owned(),local_t);
                }
              },
              None => {
                local_t.status = Some(TopicStatus{
                  pendingMessages: tinfo.pending_messages,
                  subscribers: tinfo.subscribers,
                  durables: tinfo.durables});
                t = Some(local_t.clone());  
                res.insert(tinfo.topic_name.to_owned(),local_t);
              },
            }
          },
          None => {},
        }
      }
      match t {
        Some(mut local_topic) => {
          let obj_name = get_obj_name_from_topic(&local_topic);
          info!("updating topic status for {}",obj_name);
          let updater: Api<Topic> = get_topic_client().await;
          let latest_topic: Topic = updater.get(&obj_name).await.unwrap();
          local_topic.metadata.resource_version=Meta::resource_ver(&latest_topic);
          let q_json = serde_json::to_string(&local_topic).unwrap();
          let pp = PostParams::default();
          let result = updater.replace_status(&obj_name, &pp, q_json.as_bytes().to_vec()).await;
          match result {
            Ok(_ignore) => {},
            Err(err) => {
              error!("error while updating topic object");
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

async fn get_topic_client() -> Api<Topic>{
  let config = Config::infer().await.unwrap();
  let service = Service::try_from(config).unwrap();
  let client: kube::Client = Client::new(service);
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  let crds: Api<Topic> = Api::namespaced(client, &namespace);
  return crds;
}
fn get_topic_name(topic: &Topic) -> String {
  let mut tname: String = String::from("");
  //check for name in spec
  match &topic.spec.name {
    Some(q) => tname=q.to_owned(),
    None =>{
      match &topic.metadata.name {
        Some(n) =>{
          tname = n.to_owned();
          tname.make_ascii_uppercase();
        },
        _ => {},
      }
    },
  }
  return tname;
}
fn get_obj_name_from_topic(topic: &Topic) -> String {
  return topic.metadata.name.clone().unwrap();
}

fn create_topic(topic: &mut  Topic){
  let topic_name = get_topic_name(topic);
  let script = "create topic ".to_owned()+&topic_name;
  info!("script: {}",script);
  let _result = ems::run_tibems_script(script);
  //propagate defaults
  match topic.spec.maxmsgs {
    None => topic.spec.maxmsgs=Some(0),
    _ => {},
  }
  match topic.spec.expiration {
    None => topic.spec.expiration=Some(0),
    _ => {},
  }
  topic.spec.overflowPolicy=Some(0);
  topic.spec.prefetch=Some(0);
  topic.spec.global=Some(false);
  topic.spec.maxbytes=Some(0);
  let status = TopicStatus{pendingMessages: 0,subscribers: 0,durables: 0};
  topic.status =  Some(status);
}

fn delete_topic(topic: Topic){
  let topic_name = get_topic_name(&topic);
  let script = "delete topic ".to_owned()+&topic_name+&"\n";
  info!("script: {}",script);
  let _result = ems::run_tibems_script(script);
}