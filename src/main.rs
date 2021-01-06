use tokio::task;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Result, Server, StatusCode};
use hyper::header::CONTENT_TYPE;

mod ems;
mod queue;
mod topic;
mod bridge;

#[macro_use]
extern crate log;

async fn respond(_req: Request<Body>) -> Result<Response<Body>> {
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
}

#[tokio::main]
async fn main() -> Result<()>  {
  env_logger::builder()
    .format_timestamp(None)
    .init();
  info!("starting tibco-ems-operator");

  //watch custom resource objects
  let _ignore = task::spawn(queue::watch_queues());
  let _ignore = task::spawn(topic::watch_topics());
  let _ignore = task::spawn(bridge::watch_bridges());

  //watch object statistics
  let _ignore = task::spawn(queue::watch_queues_status());
  let _ignore = task::spawn(topic::watch_topics_status());

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
  Ok(())
}