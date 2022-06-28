use axum::{
  routing::get,
  http::{StatusCode, Uri},
  http::HeaderMap,
  response::IntoResponse,
  Json, Router,
};
use tibco_ems::admin::{QueueInfo, TopicInfo};
use tibco_ems::Session;
use std::panic;
use std::process;
use urlencoding::decode;

mod queue;
mod topic;
mod bridge;
mod scaler;

#[macro_use]
extern crate log;
#[macro_use]
extern crate env_var;

async fn get_queue_stats(
  uri: Uri,
) -> impl IntoResponse {
  let uri = uri.path();
  trace!("{uri}");
  let prefix_rm_uri = uri.strip_prefix("/queue/").unwrap();
  let queue_name: String = decode(prefix_rm_uri).unwrap().into_owned();
  if queue_name.contains('|') {
    //multiple queues
    let queue_list: Vec<&str> = queue_name.split('|').collect();
    let all_queues = &mut QueueInfo{
      name: "mixed".to_string(),
      pending_messages: Some(0),
      consumer_count: Some(0),
      ..Default::default()
    };
    //get queues 
    let c_map = queue::QUEUES.lock().unwrap();
    let mut pending_messages = 0;
    let mut consumer_count = 0;
    for key in c_map.keys() {
      let qinfo = c_map.get(key).unwrap();
      for q in &queue_list {
        if q == &qinfo.name {
          pending_messages += qinfo.pending_messages.unwrap();
          consumer_count += qinfo.consumer_count.unwrap();
        }
      }
    }
    all_queues.pending_messages = Some(pending_messages);
    all_queues.consumer_count = Some(consumer_count);
    (StatusCode::OK, Json(all_queues.clone()))
  } else {
    //get single queue
    let c_map = queue::QUEUES.lock().unwrap();
    for key in c_map.keys() {
      let qinfo = c_map.get(key).unwrap();
      if qinfo.name == queue_name {
        return (StatusCode::OK, Json(qinfo.clone()));
      }
    }
    //return default if nothing is found
    let queue_info = QueueInfo{
      name: queue_name,
      pending_messages: Some(0),
      consumer_count: Some(0),
      ..Default::default()
    };
    (StatusCode::OK, Json(queue_info))
  }
}

async fn get_topic_stats(
  uri: Uri,
) -> impl IntoResponse {
  let uri = uri.path();
  trace!("{uri}");
  let prefix_rm_uri = uri.strip_prefix("/topic/").unwrap();
  let topic_name: String = decode(prefix_rm_uri).unwrap().into_owned();
  if topic_name.contains('|') {
    //multiple topics
    let topic_list: Vec<&str> = topic_name.split('|').collect();
    let all_topics = &mut TopicInfo{
      name: "mixed".to_string(),
      pending_messages: Some(0),
      subscriber_count: Some(0),
      durable_count: Some(0),
      ..Default::default()
    };
    //get topics 
    let c_map = topic::TOPICS.lock().unwrap();
    let mut pending_messages = 0;
    let mut subscriber_count = 0;
    let mut durable_count = 0;
    for key in c_map.keys() {
      let tinfo = c_map.get(key).unwrap();
      for t in &topic_list {
        if t == &tinfo.name {
          pending_messages += tinfo.pending_messages.unwrap();
          subscriber_count += tinfo.subscriber_count.unwrap();
          durable_count += tinfo.durable_count.unwrap();
        }
      }
    }
    all_topics.pending_messages = Some(pending_messages);
    all_topics.subscriber_count = Some(subscriber_count);
    all_topics.durable_count = Some(durable_count);
    (StatusCode::OK, Json(all_topics.clone()))
  } else {
    //get single topic 
    let c_map = topic::TOPICS.lock().unwrap();
    for key in c_map.keys() {
      let tinfo = c_map.get(key).unwrap();
      if tinfo.name == topic_name {
        return (StatusCode::OK, Json(tinfo.clone()));
      }
    }
    //return default if nothing is found
    let topic_info = &mut TopicInfo{
      name: topic_name,
      pending_messages: Some(0),
      subscriber_count: Some(0),
      durable_count: Some(0),
      ..Default::default()
    };
    (StatusCode::OK, Json(topic_info.clone()))
  }
}

async fn get_metrics(
  uri: Uri,
) -> impl IntoResponse {
  let uri = uri.path();
  trace!("{uri}");
  let mut body = "".to_owned();
  body.push_str("# TYPE Q:pendingMessages gauge\n");
  body.push_str("# TYPE Q:consumers gauge\n");
  body.push_str("# TYPE T:pendingMessages gauge\n");
  body.push_str("# TYPE T:subscribers gauge\n");
  body.push_str("# TYPE T:durables gauge\n");
  //get queues 
  let c_map = queue::QUEUES.lock().unwrap();
  for key in c_map.keys() {
    let qinfo = c_map.get(key).unwrap();
    let pending = format!("Q:pendingMessages{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.name,qinfo.pending_messages.unwrap());
    let consumers = format!("Q:consumers{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.name,qinfo.consumer_count.unwrap());
    body.push_str(&pending);
    body.push_str(&consumers);
  }
  //get topics 
  let c_map = topic::TOPICS.lock().unwrap();
  for key in c_map.keys() {
    let tinfo = c_map.get(key).unwrap();
    let pending = format!("T:pendingMessages{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.name,tinfo.pending_messages.unwrap());
    let subscribers = format!("T:subscribers{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.name,tinfo.subscriber_count.unwrap());
    let durables = format!("T:durables{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.name,tinfo.durable_count.unwrap());
    body.push_str(&pending);
    body.push_str(&subscribers);
    body.push_str(&durables);
  }
  let mut headers = HeaderMap::new();
  headers.insert("Content-Type", "text/plain; version=0.0.4; charset=utf-8".parse().unwrap());
  (StatusCode::OK, headers, body)
}

pub fn init_admin_connection() -> Session{
  let username = env_var!(required "USERNAME");
  let password = env_var!(required "PASSWORD");
  let server_url = env_var!(required "SERVER_URL");
  let conn = tibco_ems::admin::connect(&server_url, &username, &password).unwrap();
  info!("creating admin connection");
  conn.session().unwrap()
}

async fn api() -> String {
  "tibco-ems-operator".to_string()
}

#[tokio::main]
async fn main() {
  env_logger::init();
  info!("starting tibco-ems-operator");
  // look for responsible settings
  let responsible = env_var!(optional "RESPONSIBLE_FOR");
  if !responsible.is_empty() {
    info!("RESPONSIBLE_FOR {responsible} => only object for that instance will be monitored");
  }

  //add panic hook to shutdown engine on error
  let orig_hook = panic::take_hook();
  panic::set_hook(Box::new(move |panic_info| {
      error!("receiving panic hook, shutting down engine");
      orig_hook(panic_info);
      process::exit(1);
  }));

  let read_only = env_var!(optional "READ_ONLY", default:"FALSE");
  if read_only == "FALSE" {
    //watch custom resource objects
    let _ignore = tokio::spawn(queue::watch_queues());
    let _ignore = tokio::spawn(topic::watch_topics());
    let _ignore = tokio::spawn(bridge::watch_bridges());
  }
  
  //watch object statistics
  let _ignore = tokio::spawn(queue::watch_queues_status());
  let _ignore = tokio::spawn(topic::watch_topics_status());

  let scaling = env_var!(optional "ENABLE_SCALING", default:"FALSE");
  if scaling == "TRUE" {
    //watch custom resource objects
    let _ignore = tokio::spawn(scaler::run());
  }

  //watch for shutdown signal
  tokio::spawn(sighup());

  //spawn metrics server
  let addr = "0.0.0.0:8080".parse().unwrap();
  let app = Router::new()
      .route("/", get(api))
      .route("/queue/:queuename", get(get_queue_stats))
      .route("/topic/:topicname", get(get_topic_stats))
      .route("/metrics", get(get_metrics));

  info!("listening on {}", addr);
  axum::Server::bind(&addr)
      .serve(app.into_make_service())
      .await
      .unwrap();
      

  std::thread::park();
  info!("done");
}

#[cfg(not(feature = "windows"))]
async fn sighup() {
  let mut stream = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
  loop {
      stream.recv().await;
      info!("got SIGTERM, shutting down");
      std::process::exit(0);
  }
}

#[cfg(feature = "windows")]
async fn sighup() {
  tokio::signal::ctrl_c().await.unwrap();
  info!("got SIGTERM, shutting down");
  std::process::exit(0);
}
