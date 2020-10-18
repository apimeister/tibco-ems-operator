use kube::{api::{Api, ListParams, Meta, PostParams}, Client};
use kube_runtime::watcher;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;
use std::env;
use tokio::time::{self, Duration};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use hyper::Result;

#[path = "../ems/mod.rs"]
mod ems;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct TopicStatus {
  pub pendingMessages: i64,
  pub subscribers: i32,
  pub durables: i32,
}

pub static KNOWN_TOPICS: Lazy<Mutex<HashMap<String, Topic>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

pub async fn watch_topics() -> Result<()>{
  let crds: Api<Topic> = get_topic_client().await;
  let updater: Api<Topic> = crds.clone();

  let lp = ListParams::default();
  println!("subscribing events of type topics.tibcoems.apimeister.com/v1");
  let mut stream = watcher(crds, lp).boxed();
  while let Some(status) = stream.try_next().await.unwrap() {
    match status {
      kube_runtime::watcher::Event::Applied(mut topic) =>{
        let topic_name = get_topic_name(&topic);
        let name = Meta::name(&topic);
        {
          let mut res = KNOWN_TOPICS.lock().unwrap();
          match res.get(&topic_name) {
            Some(_queue) => println!("queue already known {}", &topic_name),
            None => {
              println!("adding queue {}", &topic_name);
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
      }
      kube_runtime::watcher::Event::Deleted(topic) =>{
        delete_topic(topic);
      },
      kube_runtime::watcher::Event::Restarted(topics) =>{
        let mut res = KNOWN_TOPICS.lock().unwrap();
        for (idx, topic) in topics.iter().enumerate() {
          let topic_name = get_topic_name(topic);
          println!("{}: adding topic to monitor {}",idx+1,topic_name);
          res.insert(topic_name.to_owned(), topic.clone());
        }
      },
    }
  }
  println!("finished watching topics");
  Ok(())
}
pub async fn watch_topics_status() -> Result<()>{
  let status_refresh_in_ms = env::var("STATUS_REFRESH_IN_MS");
  let mut interval: u64  = 10000;
  match status_refresh_in_ms {
    Ok(val) => interval=val.parse().unwrap(),
    Err(_error) => {},
  }
  let mut interval = time::interval(Duration::from_millis(interval));
  let re = Regex::new(r"[\*]?\s*(?P<name>[\$\.\S]*)\s*(?P<flags>.[+-]*)\s*(?P<subscribers>[\d\*]*)\s*(?P<durables>[\d]*)\s*(?P<pendingMsgs>[\d]*)\s*(?P<pendingBytes>[\d\.]*)\s.*").unwrap();
  loop {
    let result = ems::run_tibems_script("show topics".to_owned());
    let lines: Vec<&str> = result.split("\n").collect();
    for (index, line) in lines.iter().enumerate() {
      //skip first 10 lines
      if index> 10 {
        let values = re.captures(line);
        match values {
          Some(vec) =>{
            let tname = &vec["name"];
            let mut t : Option<Topic> = None;
            {
              let mut res = KNOWN_TOPICS.lock().unwrap();
              match res.get(tname) {
                Some(topic) =>{
                  let mut local_topic = topic.clone();
                  let pending_msgs: i64 = *&vec["pendingMsgs"].to_owned().parse().unwrap();
                  let subscribers: i32 = *&vec["subscribers"].to_owned().parse().unwrap();
                  let durables: i32 = *&vec["durables"].to_owned().parse().unwrap();
                  match &topic.status {
                    Some(status) => {
                      if status.pendingMessages != pending_msgs 
                        || status.subscribers != subscribers
                        || status.durables != durables {
                        local_topic.status = Some(TopicStatus{
                            pendingMessages: pending_msgs,
                            subscribers: subscribers,
                            durables: durables});
                        t = Some(local_topic.clone());  
                        res.insert(tname.to_owned(),local_topic);
                      }
                    },
                    None => {
                      local_topic.status = Some(TopicStatus{
                        pendingMessages: pending_msgs,
                        subscribers: subscribers,
                        durables: durables});
                      t = Some(local_topic.clone());  
                      res.insert(tname.to_owned(),local_topic);
                    },
                  }
                },
                None => {},
              }
            }
            match t {
              Some(mut local_topic) => {
                println!("setting topic status ");
                let obj_name = get_obj_name_from_topic(&local_topic);
                let updater: Api<Topic> = get_topic_client().await;
                let latest_topic: Topic = updater.get(&obj_name).await.unwrap();
                local_topic.metadata.resource_version=Meta::resource_ver(&latest_topic);
                let q_json = serde_json::to_string(&local_topic).unwrap();
                let pp = PostParams::default();
                let _result = updater.replace_status(&obj_name, &pp, q_json.as_bytes().to_vec()).await;
              },
              None => {},
            }
          },
          None => {},
        }
      }
    }
    interval.tick().await;
  }
}

async fn get_topic_client() -> Api<Topic>{
  let config = Config::infer().await.unwrap();
  let client: kube::Client = Client::new(config);
  let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
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
  println!("script: {}",script);
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
  println!("script: {}",script);
  let _result = ems::run_tibems_script(script);
}