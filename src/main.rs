use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Result, Server, StatusCode};
use hyper::header::CONTENT_TYPE;
use tibco_ems::admin::{QueueInfo, TopicInfo};
use tibco_ems::Session;
use std::panic;
use std::process;

mod queue;
mod topic;
mod bridge;
mod scaler;

#[macro_use]
extern crate log;
#[macro_use]
extern crate env_var;

async fn respond(req: Request<Body>) -> Result<Response<Body>> {
  let uri = req.uri().path();
  trace!("{} {}",req.method(),uri);

// sort by usage, queue scaler, most used
  let response = match uri {
    x if x.starts_with("/queue/") => {
      let prefix_rm_uri = uri.strip_prefix("/queue/").unwrap();
      let mut json_string;
      let queue_name: String = prefix_rm_uri.replace("%7C", "|");
      if queue_name.contains("|") {
        //multiple queues
        let queue_list: Vec<&str> = queue_name.split("|").collect();
        let all_queues = &mut QueueInfo{
          name: "mixed".to_string(),
          pending_messages: Some(0),
          consumer_count: Some(0),
          ..Default::default()
        };
        //get queues 
        {
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
        }
        json_string = serde_json::to_string(all_queues).unwrap();
      } else {
        //get single queue
        let queue_info = &mut QueueInfo{
          name: queue_name.to_string(),
          pending_messages: Some(0),
          consumer_count: Some(0),
          ..Default::default()
        };
        json_string = serde_json::to_string(queue_info).unwrap();
        {
          let c_map = queue::QUEUES.lock().unwrap();
          for key in c_map.keys() {
            let qinfo = c_map.get(key).unwrap();
            if qinfo.name == queue_name {
              json_string = serde_json::to_string(qinfo).unwrap();
            }
          }
        }
      }
      Response::builder()
      .status(StatusCode::OK)
      .header(CONTENT_TYPE, "application/json; charset=utf-8")
      .body(Body::from(json_string))
      .unwrap()
    },
    "/metrics" => {
      let mut body = "".to_owned();
      body.push_str("# TYPE Q:pendingMessages gauge\n");
      body.push_str("# TYPE Q:consumers gauge\n");
      body.push_str("# TYPE T:pendingMessages gauge\n");
      body.push_str("# TYPE T:subscribers gauge\n");
      body.push_str("# TYPE T:durables gauge\n");
      //get queues 
      {
        let c_map = queue::QUEUES.lock().unwrap();
        for key in c_map.keys() {
          let qinfo = c_map.get(key).unwrap();
          let pending = format!("Q:pendingMessages{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.name,qinfo.pending_messages.unwrap());
          let consumers = format!("Q:consumers{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.name,qinfo.consumer_count.unwrap());
          body.push_str(&pending);
          body.push_str(&consumers);
        }
      }
      //get topics 
      {
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
      }
      Response::builder()
          .status(StatusCode::OK)
          .header(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")
          .body(Body::from(body))
          .unwrap()
    },
    x if x.starts_with("/topic/") => {
      let prefix_rm_uri = uri.strip_prefix("/topic/").unwrap();
      let mut json_string;
      let topic_name: String = prefix_rm_uri.replace("%7C", "|");
      if topic_name.contains("|") {
        //multiple topics
        let topic_list: Vec<&str> = topic_name.split("|").collect();
        let all_topics = &mut TopicInfo{
          name: "mixed".to_string(),
          pending_messages: Some(0),
          subscriber_count: Some(0),
          durable_count: Some(0),
          ..Default::default()
        };
        //get topics 
        {
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
        }
        json_string = serde_json::to_string(all_topics).unwrap();
      } else {
        //get single topic 
        let topic_info = &mut TopicInfo{
          name: topic_name.to_string(),
          pending_messages: Some(0),
          subscriber_count: Some(0),
          durable_count: Some(0),
          ..Default::default()
        };
        json_string = serde_json::to_string(topic_info).unwrap();
        {
          let c_map = topic::TOPICS.lock().unwrap();
          for key in c_map.keys() {
            let tinfo = c_map.get(key).unwrap();
            if tinfo.name == topic_name {
              json_string = serde_json::to_string(tinfo).unwrap();
            }
          }
        }
      }
      Response::builder()
      .status(StatusCode::OK)
      .header(CONTENT_TYPE, "application/json; charset=utf-8")
      .body(Body::from(json_string))
      .unwrap()
    },
    _ => {
      error!("unkown endpoint: {}",uri);
      Response::builder()
          .status(StatusCode::NOT_FOUND)
          .header(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")
          .body(Body::from(""))
          .unwrap()
    }
  };
  Ok(response)
}

pub fn init_admin_connection() -> Session{
  let username = env_var!(required "USERNAME");
  let password = env_var!(required "PASSWORD");
  let server_url = env_var!(required "SERVER_URL");
  let conn = tibco_ems::admin::connect(&server_url, &username, &password).unwrap();
  info!("creating admin connection");
  conn.session().unwrap()
}

#[tokio::main]
async fn main() {
  env_logger::init();
  info!("starting tibco-ems-operator");

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

  //spawn metrics server
  let addr = "0.0.0.0:8080".parse().unwrap();
  let make_service = make_service_fn(|_|
      async { Ok::<_, hyper::Error>(service_fn(respond)) });
  let server = Server::bind(&addr).serve(make_service);
  info!("Listening on http://{}", addr);
  if let Err(e) = server.await {
      error!("server error: {}", e);
  }

  std::thread::park();
  info!("done");
}