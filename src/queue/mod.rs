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
    kind="Queue",
    status="QueueStatus",
    namespaced)]
#[allow(non_snake_case)]
pub struct QueueSpec {
  pub name: Option<String>,
  pub expiration: Option<u32>,
  pub global: Option<bool>,
  pub maxbytes: Option<u32>,
  pub maxmsgs: Option<u32>,
  pub maxRedelivery: Option<u32>,
  pub overflowPolicy: Option<u8>,
  pub prefetch: Option<u32>,
  pub redeliveryDelay: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct QueueStatus {
  pub pendingMessages: i64,
  pub consumerCount: i32,
}

pub static KNOWN_QUEUES: Lazy<Mutex<HashMap<String, Queue>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

pub async fn watch_queues() -> Result<()>{
  let crds: Api<Queue> = get_queue_client().await;
  let updater: Api<Queue> = crds.clone();
  
  let lp = ListParams::default();
  println!("subscribing events of type queues.tibcoems.apimeister.com/v1");
  let mut stream = watcher(crds, lp).boxed();
  while let Some(status) = stream.try_next().await.unwrap() {
    match status {
      kube_runtime::watcher::Event::Applied(mut queue) =>{
        let qname = get_queue_name(&queue);
        let name = Meta::name(&queue);
        {
          let mut res = KNOWN_QUEUES.lock().unwrap();
          match res.get(&qname) {
            Some(_queue) => println!("queue already known {}", &qname),
            None => {
              println!("adding queue {}", &qname);
              create_queue(&mut queue);
              let q = (&queue).clone();
              let n = (&qname).clone();
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
      }
      kube_runtime::watcher::Event::Deleted(queue) =>{
        delete_queue(queue.clone());
        let mut res = KNOWN_QUEUES.lock().unwrap();
        let qname = get_queue_name(&queue);
        res.remove(&qname);           
      },
      kube_runtime::watcher::Event::Restarted(queues) =>{
        let mut res = KNOWN_QUEUES.lock().unwrap();
        for (idx, queue) in queues.iter().enumerate() {
          let queue_name = get_queue_name(queue);
          let obj_name = get_obj_name_from_queue(queue);
          if queue_name != obj_name {
            println!("{}: adding queue to monitor {}",idx+1,queue_name);
          }else{
            println!("{}: adding queue to monitor {} ({})",idx+1,queue_name,obj_name);
          }
          res.insert(queue_name.to_owned(), queue.clone()); 
        }
      },
    }
  }
  println!("finished watching queues");
  Ok(())
}

pub async fn watch_queues_status() -> Result<()>{
  let status_refresh_in_ms = env::var("STATUS_REFRESH_IN_MS");
  let mut interval: u64  = 10000;
  match status_refresh_in_ms {
    Ok(val) => interval=val.parse().unwrap(),
    Err(_error) => {},
  }
  let mut interval = time::interval(Duration::from_millis(interval));
  let re = Regex::new(r"[\*]?\s*(?P<name>[\$\.\S]*)\s*(?P<flags>.[+-]*)\s*(?P<prefetch>[\d\*]*)\s*(?P<consumers>[\d]*)\s*(?P<pendingMsgs>[\d]*)\s*(?P<pendingBytes>[\d\.]*)\s.*").unwrap();
  loop {
    let result = ems::run_tibems_script("show queues".to_owned());
    let lines: Vec<&str> = result.split("\n").collect();
    for (index, line) in lines.iter().enumerate() {
      //skip first 10 lines
      if index> 10 {
        let values = re.captures(line);
        match values {
          Some(vec) =>{
            let qname = &vec["name"];
            let mut q : Option<Queue> = None;
            {
              let mut res = KNOWN_QUEUES.lock().unwrap();
              match res.get(qname) {
                Some(queue) =>{
                  let mut local_q = queue.clone();
                  // let local_q = 
                  let pending_msgs: i64 = *&vec["pendingMsgs"].to_owned().parse().unwrap();
                  let consumers: i32 = *&vec["consumers"].to_owned().parse().unwrap();
                  match &queue.status {
                    Some(status) => {
                      if status.pendingMessages != pending_msgs 
                        || status.consumerCount != consumers {
                        local_q.status = Some(QueueStatus{
                            pendingMessages: pending_msgs,
                            consumerCount: consumers});
                        q = Some(local_q.clone());  
                        res.insert(qname.to_owned(),local_q);
                      }
                    },
                    None => {
                      local_q.status = Some(QueueStatus{
                        pendingMessages: pending_msgs,
                        consumerCount: consumers});
                      q = Some(local_q.clone());  
                      res.insert(qname.to_owned(),local_q);
                    },
                  }
                },
                None => {},
              }
            }
            match q {
              Some(mut local_q) => {
                println!("setting queue status ");
                let obj_name = get_obj_name_from_queue(&local_q);
                let updater: Api<Queue> = get_queue_client().await;
                let latest_queue: Queue = updater.get(&obj_name).await.unwrap();
                local_q.metadata.resource_version=Meta::resource_ver(&latest_queue);
                let q_json = serde_json::to_string(&local_q).unwrap();
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

async fn get_queue_client() -> Api<Queue>{
  let config = Config::infer().await.unwrap();
  let client: kube::Client = Client::new(config);
  let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
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
  let script = "create queue ".to_owned()+&qname+"\n";
  println!("script: {}",script);
  let _result = ems::run_tibems_script(script);
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
  println!("deleting queue {}", queue_name);
  let script = "delete queue ".to_owned()+&queue_name+"\n";
  let _result = ems::run_tibems_script(script);
}