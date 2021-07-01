use kube::{api::{Api, ListParams, ResourceExt, PostParams}, Client};
use kube::api::WatchEvent;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use tokio::time::{self, Duration};
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use hyper::Result;
use schemars::JsonSchema;
use env_var::env_var;
use tibco_ems::admin::TopicInfo;
use tibco_ems::Session;

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
  pub maxbytes: Option<i64>,
  pub maxmsgs: Option<i64>,
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

pub static TOPICS: Lazy<Mutex<HashMap<String,TopicInfo>>> = Lazy::new(|| 
  Mutex::new(HashMap::new() ) );

///used for retrieving queue statistics
static TOPIC_ADMIN_CONNECTION: Lazy<Mutex<Session>> = Lazy::new(|| Mutex::new(super::init_admin_connection()));
///used for sending admin operations
static ADMIN_CONNECTION: Lazy<Mutex<Session>> = Lazy::new(|| Mutex::new(super::init_admin_connection()));

pub async fn watch_topics() -> Result<()>{
  let crds: Api<Topic> = get_topic_client().await;
  let updater: Api<Topic> = crds.clone(); 
  let lp = ListParams::default();

  let mut last_version: String = "0".to_owned();
  info!("subscribing events of type topics.tibcoems.apimeister.com/v1");
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
                    WatchEvent::Added(mut topic) =>{
                      let topic_name = get_topic_name(&topic);
                      let name = ResourceExt::name(&topic);
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
                      last_version = ResourceExt::resource_version(&topic).unwrap();
                    },
                    WatchEvent::Deleted(topic) =>{
                      let do_not_delete = env_var!(optional "DO_NOT_DELETE_OBJECTS", default:"FALSE");
                      let tname = get_topic_name(&topic);
                      if do_not_delete == "TRUE" {
                        warn!("delete event for {} (not executed because of DO_NOT_DELETE_OBJECTS setting)",tname);
                      }else{
                        delete_topic(&topic);
                      }
                      last_version = ResourceExt::resource_version(&topic).unwrap();
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

pub async fn watch_topics_status() -> Result<()>{
  let status_refresh_in_ms: u64 = env_var!(optional "STATUS_REFRESH_IN_MS", default: "10000").parse().unwrap();
  let read_only = env_var!(optional "READ_ONLY", default:"FALSE");
  let mut interval = time::interval(Duration::from_millis(status_refresh_in_ms));
  loop {
    let result;
    {
      let session = TOPIC_ADMIN_CONNECTION.lock().unwrap();
      result = tibco_ems::admin::list_all_topics(&session);
    }
    match result {
      Ok(res) => {
        for tinfo in &res {
          //update prometheus
          {
            let mut c_map = TOPICS.lock().unwrap();
            c_map.insert(tinfo.name.clone(),tinfo.clone());
          }
          //update k8s state
          if read_only == "FALSE" {
            let mut t : Option<Topic> = None;
            {
              let mut res = KNOWN_TOPICS.lock().unwrap();
              match res.get(&tinfo.name) {
                Some(topic) =>{
                  let mut local_t = topic.clone();
                  match &topic.status {
                    Some(status) => {
                      if status.pendingMessages != tinfo.pending_messages.unwrap()
                        || status.subscribers != tinfo.subscriber_count.unwrap()
                        || status.durables != tinfo.durable_count.unwrap() {
                        local_t.status = Some(TopicStatus{
                            pendingMessages: tinfo.pending_messages.unwrap(),
                            subscribers: tinfo.subscriber_count.unwrap(),
                            durables: tinfo.durable_count.unwrap() });
                        t = Some(local_t.clone());  
                        res.insert(tinfo.name.to_owned(),local_t);
                      }
                    },
                    None => {
                      local_t.status = Some(TopicStatus{
                        pendingMessages: tinfo.pending_messages.unwrap(),
                        subscribers: tinfo.subscriber_count.unwrap(),
                        durables: tinfo.durable_count.unwrap()});
                      t = Some(local_t.clone());  
                      res.insert(tinfo.name.to_owned(),local_t);
                    },
                  }
                },
                None => {},
              }
            }
            match t {
              Some(mut local_topic) => {
                let obj_name = get_obj_name_from_topic(&local_topic);
                debug!("updating topic status for {}",obj_name);
                let updater: Api<Topic> = get_topic_client().await;
                let latest_topic: Topic = updater.get(&obj_name).await.unwrap();
                local_topic.metadata.resource_version=ResourceExt::resource_version(&latest_topic);
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
        }
      },
      Err(_err) => {
        panic!("failed to retrieve topic information");
      }
    }
    interval.tick().await;
  }
}

async fn get_topic_client() -> Api<Topic>{
  let client = Client::try_default().await.expect("getting default client");
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
  let tname = get_topic_name(topic);

  let mut topic_info = TopicInfo{
    name: tname,
    max_bytes: topic.spec.maxbytes,
    max_messages: topic.spec.maxmsgs,
    global: topic.spec.global,
    ..Default::default()
  };
  match topic.spec.expiration {
    Some(val) => {
      topic_info.expiry_override = Some(val as i64);
    },
    None => {},
  }
  let session = ADMIN_CONNECTION.lock().unwrap();
  tibco_ems::admin::create_topic(&session, &topic_info);

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

fn delete_topic(topic: &Topic){
  let tname = get_topic_name(topic);
  info!("deleting topic {}", tname);

  let session = ADMIN_CONNECTION.lock().unwrap();
  tibco_ems::admin::delete_topic(&session, &tname);
}