extern crate serde_derive;
use kube::{api::{Api, ListParams}, Client};
use kube_runtime::watcher;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use kube_derive::CustomResource;
use kube::config::Config;
use std::process::Command;
use std::process::Child;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use tokio::task;

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", kind="Queue", namespaced)]
#[allow(non_snake_case)]
pub struct QueueSpec {
    pub expiration: Option<u32>,
    pub global: Option<bool>,
    pub maxbytes: Option<u32>,
    pub maxmsgs: Option<u32>,
    pub maxRedelivery: Option<u32>,
    pub overflowPolicy: Option<u8>,
    pub prefetch: Option<u32>,
    pub redeliveryDelay: Option<u32>,
}

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", kind="Topic", namespaced)]
#[allow(non_snake_case)]
pub struct TopicSpec {
    pub expiration: Option<u32>,
    pub global: Option<bool>,
    pub maxbytes: Option<u32>,
    pub maxmsgs: Option<u32>,
    pub overflowPolicy: Option<u8>,
    pub prefetch: Option<u32>,
}

#[derive(CustomResource, Serialize, Deserialize, Default, Clone, Debug)]
#[kube(group = "tibcoems.apimeister.com", version = "v1", kind="Bridge", namespaced)]
#[allow(non_snake_case)]
pub struct BridgeSpec {
    pub source_type: String,
    pub source_name: String,
    pub target_type: String,
    pub target_name: String,
    pub selector: Option<String>,
}

async fn watch_queues() -> Result<(),kube_runtime::watcher::Error>{
    let config = Config::infer().await;
    match config {
        Err(e) => println!("error {}", e),
        Ok(c) => {
          let client: kube::Client = Client::new(c);
          let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
          let crds: Api<Queue> = Api::namespaced(client, &namespace);
          let lp = ListParams::default();

          println!("subscribing events of type queues.tibcoems.apimeister.com/v1");
          let mut stream = watcher(crds, lp).boxed();
          while let Some(status) = stream.try_next().await? {
              match status {
                kube_runtime::watcher::Event::Applied(queue) =>{
                  create_queue(queue);
                }
                kube_runtime::watcher::Event::Deleted(queue) =>{
                  delete_queue(queue);
                },
                kube_runtime::watcher::Event::Restarted(_queue) =>{
                  println!("restart queue event not implemented");
                },
              }
          }
        }
      }
    println!("finished watching queues");
    Ok(())
}

async fn watch_topics() -> Result<(),kube_runtime::watcher::Error>{
  let config = Config::infer().await;
  match config {
      Err(e) => println!("error {}", e),
      Ok(c) => {
        let client: kube::Client = Client::new(c);
        let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
        let crds: Api<Topic> = Api::namespaced(client, &namespace);
        let lp = ListParams::default();

        println!("subscribing events of type topics.tibcoems.apimeister.com/v1");
        let mut stream = watcher(crds, lp).boxed();
        while let Some(status) = stream.try_next().await? {
          match status {
            kube_runtime::watcher::Event::Applied(topic) =>{
              create_topic(topic);
            }
            kube_runtime::watcher::Event::Deleted(topic) =>{
              delete_topic(topic);
            },
            kube_runtime::watcher::Event::Restarted(_topic) =>{
              println!("restart topic event not implemented");
            },
          }
        }
      }
    }
  println!("finished watching topics");
  Ok(())
}

async fn watch_bridges() -> Result<(),kube_runtime::watcher::Error>{
  let config = Config::infer().await;
  match config {
      Err(e) => println!("error {}", e),
      Ok(c) => {
        let client: kube::Client = Client::new(c);
        let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
        let crds: Api<Bridge> = Api::namespaced(client, &namespace);
        let lp = ListParams::default();

        println!("subscribing events of type bridges.tibcoems.apimeister.com/v1");
        let mut stream = watcher(crds, lp).boxed();
        while let Some(status) = stream.try_next().await? {
          match status {
            kube_runtime::watcher::Event::Applied(bridge) =>{
              create_bridge(bridge);
            }
            kube_runtime::watcher::Event::Deleted(bridge) =>{
              delete_bridge(bridge);
            },
            kube_runtime::watcher::Event::Restarted(_bridge) =>{
              println!("restart bridge event not implemented");
            },
          }
        }
      }
    }
  println!("finished watching bridges");
  Ok(())
}

fn write_script_file(script: String) {
  let mut file = File::create("/tmp/ems.script").unwrap();
  file.write_all(&script.into_bytes()).unwrap();
}

fn run_tibemsadmin() -> Child{
  let username = env::var("USERNAME").unwrap();
  let password = env::var("PASSWORD").unwrap();
  let server_url = env::var("SERVER_URL").unwrap();

  let p = Command::new("tibemsadmin")
      .arg("-user")
      .arg(username)
      .arg("-password")
      .arg(password)
      .arg("-server")
      .arg(server_url)
      .arg("-module_path")
      .arg("/usr/lib64")
      .arg("-script")
      .arg("/tmp/ems.script")
      .spawn().unwrap();
  return p;
}

fn create_queue(queue: Queue){
  let mut qname: String = queue.metadata.name.unwrap();
  qname.make_ascii_uppercase();
  let script = "create queue ".to_owned()+&qname+"\n";
  println!("script: {}",script);
  write_script_file(script);
  let child = run_tibemsadmin();

  println!("{:?}",child.wait_with_output());
}

fn delete_queue(queue: Queue){
  let mut qname: String = queue.metadata.name.unwrap();
  qname.make_ascii_uppercase();
  println!("deleting queue {}", qname);
  let script = "delete queue ".to_owned()+&qname+"\n";
  println!("script: {}",script);
  write_script_file(script);
  let child = run_tibemsadmin();

  println!("{:?}",child.wait_with_output());
}

fn create_topic(topic: Topic){
  let mut topic_name: String = topic.metadata.name.unwrap();
  topic_name.make_ascii_uppercase();
  let script = "create topic ".to_owned()+&topic_name;
  println!("script: {}",script);
  write_script_file(script);
  let child = run_tibemsadmin();

  println!("{:?}",child.wait_with_output());
}

fn delete_topic(topic: Topic){
  let mut topic_name: String = topic.metadata.name.unwrap();
  topic_name.make_ascii_uppercase();
  let script = "delete topic ".to_owned()+&topic_name+&"\n";
  println!("script: {}",script);
  write_script_file(script);
  let child = run_tibemsadmin();

  println!("{:?}",child.wait_with_output());
}

fn create_bridge(bridge: Bridge){
  let bridge_name: String = bridge.metadata.name.unwrap();
  println!("creating bridge {}",bridge_name);
  let mut source_name = bridge.spec.source_name;
  let mut target_name = bridge.spec.target_name;
  source_name.make_ascii_uppercase();
  target_name.make_ascii_uppercase();
  let script = "create bridge ".to_owned()
        +"source="+&bridge.spec.source_type+":"+&source_name
        +"target="+&bridge.spec.target_type+":"+&target_name
        +&"\n";
  println!("script: {}",script);
  write_script_file(script);
  let child = run_tibemsadmin();

  println!("{:?}",child.wait_with_output());
}

fn delete_bridge(bridge: Bridge){
  let bridge_name: String = bridge.metadata.name.unwrap();
  println!("deleting bridge {}",bridge_name);
  let mut source_name = bridge.spec.source_name;
  let mut target_name = bridge.spec.target_name;
  source_name.make_ascii_uppercase();
  target_name.make_ascii_uppercase();
  let script = "delete bridge ".to_owned()
        +"source="+&bridge.spec.source_type+":"+&source_name
        +"target="+&bridge.spec.target_type+":"+&target_name
        +&"\n";
  println!("script: {}",script);
  write_script_file(script);
  let child = run_tibemsadmin();

  println!("{:?}",child.wait_with_output());
}

#[tokio::main]
async fn main() -> Result<(), kube::Error>  {
    println!("starting tibco-ems-operator");
    let _ignore = task::spawn(watch_queues());
    let _ignore = task::spawn(watch_topics());
    let _ignore = task::spawn(watch_bridges());
    std::thread::park();
    println!("done");
    Ok(())
}