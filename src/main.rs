extern crate serde_derive;
use kube::{api::{Api, ListParams, WatchEvent}, Client};
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
    pub destination_type: String,
    pub destination_name: String,
}

async fn watch_queues() -> Result<(),kube::Error>{
    let config = Config::infer().await;
    match config {
        Err(e) => println!("error {}", e),
        Ok(c) => {
            let client: kube::Client = Client::new(c);
            let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
            let crds: Api<Queue> = Api::namespaced(client, &namespace);
            let lp = ListParams::default();

            println!("streaming events queues ...");
            let mut stream = crds.watch(&lp, "0").await?.boxed();
            while let Some(status) = stream.try_next().await? {
                match status {
                    WatchEvent::Added(queue) => {
                      create_queue(queue);
                    },
                    WatchEvent::Modified(queue) => {
                      println!("modified queue event not implemented: {}",queue.metadata.name.unwrap());
                    },
                    WatchEvent::Deleted(queue) => {
                      delete_queue(queue);
                    },
                    WatchEvent::Error(queue) => println!("error: {}", queue),
                    _ => {}
                }
            }
        }
      }
    Ok(())
}

async fn watch_topics() -> Result<(),kube::Error>{
  let config = Config::infer().await;
  match config {
      Err(e) => println!("error {}", e),
      Ok(c) => {
          let client: kube::Client = Client::new(c);
          let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
          let crds: Api<Topic> = Api::namespaced(client, &namespace);
          let lp = ListParams::default();

          println!("streaming events topics ...");
          let mut stream = crds.watch(&lp, "0").await?.boxed();
          while let Some(status) = stream.try_next().await? {
              match status {
                  WatchEvent::Added(topic) => {
                    create_topic(topic);
                  },
                  WatchEvent::Modified(topic) => {
                    println!("modified topic event not implemented: {}",topic.metadata.name.unwrap());
                  },
                  WatchEvent::Deleted(topic) => {
                    delete_topic(topic);
                  },
                  WatchEvent::Error(topic) => println!("error: {}", topic),
                  _ => {}
              }
          }
      }
    }
  Ok(())
}

async fn watch_bridges() -> Result<(),kube::Error>{
  let config = Config::infer().await;
  match config {
      Err(e) => println!("error {}", e),
      Ok(c) => {
          let client: kube::Client = Client::new(c);
          let namespace = env::var("KUBERNETES_NAMESPACE").unwrap();
          let crds: Api<Bridge> = Api::namespaced(client, &namespace);
          let lp = ListParams::default();

          println!("streaming events bridges ...");
          let mut stream = crds.watch(&lp, "0").await?.boxed();
          while let Some(status) = stream.try_next().await? {
              match status {
                  WatchEvent::Added(bridge) => {
                    create_bridge(bridge);
                  },
                  WatchEvent::Modified(bridge) => {
                    println!("modified bridge event not implemented: {}",bridge.metadata.name.unwrap());
                  },
                  WatchEvent::Deleted(bridge) => {
                    delete_bridge(bridge);
                  },
                  WatchEvent::Error(bridge) => println!("error: {}", bridge),
                  _ => {}
              }
          }
      }
    }
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
  println!("create bridge source=type:dest_name target=type:dest_name [selector=msg-selector]");
}
fn delete_bridge(bridge: Bridge){
  println!("delete bridge source=type:dest_name target=type:dest_name");
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