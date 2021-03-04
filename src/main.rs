use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Result, Server, StatusCode};
use hyper::header::CONTENT_TYPE;

mod ems;
mod queue;
mod topic;
mod bridge;

#[macro_use]
extern crate log;

async fn respond(req: Request<Body>) -> Result<Response<Body>> {
  let uri = req.uri().path();
  if uri == "/metrics" {
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
        let pending = format!("Q:pendingMessages{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.queue_name,qinfo.pending_messages);
        let consumers = format!("Q:consumers{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.queue_name,qinfo.consumers);
        body.push_str(&pending);
        body.push_str(&consumers);
      }
    }
    //get topics 
    {
      let c_map = topic::TOPICS.lock().unwrap();
      for key in c_map.keys() {
        let tinfo = c_map.get(key).unwrap();
        let pending = format!("T:pendingMessages{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.topic_name,tinfo.pending_messages);
        let subscribers = format!("T:subscribers{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.durables,tinfo.subscribers);
        let durables = format!("T:durables{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.durables,tinfo.durables);
        body.push_str(&pending);
        body.push_str(&subscribers);
        body.push_str(&durables);
      }
    }
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")
        .body(Body::from(body))
        .unwrap();
    Ok(response)
  } else {
    if uri.starts_with("/queue/"){
      let queue_name = uri.strip_prefix("/queue/").unwrap();
      let mut json_string = "".to_string();
      if queue_name.contains("|") {
        //multiple queues
        let queue_list = queue_name.split("|");
        let all_queues = &mut ems::QueueInfo{
          queue_name: "mixed".to_string(),
          pending_messages: 0,
          consumers: 0,
        };
        //get queues 
        {
          let c_map = queue::QUEUES.lock().unwrap();
          for key in c_map.keys() {
            let qinfo = c_map.get(key).unwrap();
            queue_list.clone().by_ref().for_each(|e| {
              if &qinfo.queue_name == e {
                all_queues.pending_messages += qinfo.pending_messages;
                all_queues.consumers += qinfo.consumers;
              }
            });
          }
        }
        json_string = serde_json::to_string(all_queues).unwrap();
      } else {
        //get single queue
        {
          let c_map = queue::QUEUES.lock().unwrap();
          for key in c_map.keys() {
            let qinfo = c_map.get(key).unwrap();
            if qinfo.queue_name == queue_name {
              json_string = serde_json::to_string(qinfo).unwrap();
            }
          }
        }
      }
      let response = Response::builder()
      .status(StatusCode::OK)
      .header(CONTENT_TYPE, "application/json; charset=utf-8")
      .body(Body::from(json_string))
      .unwrap();
      Ok(response)
    } else {
      if uri.starts_with("/topic/") {
        let topic_name = uri.strip_prefix("/topic/").unwrap();
        let mut json_string = "".to_string();
        if topic_name.contains("|") {
          //multiple topics
          let topic_list = topic_name.split("|");
          let all_topics = &mut ems::TopicInfo{
            topic_name: "mixed".to_string(),
            pending_messages: 0,
            subscribers: 0,
            durables: 0,
          };
          //get topics 
          {
            let c_map = topic::TOPICS.lock().unwrap();
            for key in c_map.keys() {
              let tinfo = c_map.get(key).unwrap();
              topic_list.clone().by_ref().for_each(|e| {
                if &tinfo.topic_name == e {
                  all_topics.pending_messages += tinfo.pending_messages;
                  all_topics.subscribers += tinfo.subscribers;
                  all_topics.durables += tinfo.durables;
                }
              });
            }
          }
          json_string = serde_json::to_string(all_topics).unwrap();
        } else {  
          //get single topic 
          {
            let c_map = topic::TOPICS.lock().unwrap();
            for key in c_map.keys() {
              let tinfo = c_map.get(key).unwrap();
              if tinfo.topic_name == topic_name {
                json_string = serde_json::to_string(tinfo).unwrap();
              }
            }
          }
        }
        let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .body(Body::from(json_string))
        .unwrap();
        Ok(response)
      } else {
        //unknown uri
        error!("unkown endpoint: {}",uri);
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")
            .body(Body::from(""))
            .unwrap();
        Ok(response)
      }
    }
  }
}

#[tokio::main]
async fn main() {
  env_logger::builder()
    .format_timestamp(None)
    .init();
  info!("starting tibco-ems-operator");

  //watch custom resource objects
  let _ignore = tokio::spawn(queue::watch_queues());
  let _ignore = tokio::spawn(topic::watch_topics());
  let _ignore = tokio::spawn(bridge::watch_bridges());

  //watch object statistics
  let _ignore = tokio::spawn(queue::watch_queues_status());
  let _ignore = tokio::spawn(topic::watch_topics_status());

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