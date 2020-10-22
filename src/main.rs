use tokio::task;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Result, Server, StatusCode};
use hyper::header::CONTENT_TYPE;
use prometheus::TextEncoder;
use prometheus::Encoder;
use prometheus::Registry;
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub mod ems;
mod queue;
mod topic;
mod bridge;

pub static PROMETHEUS_REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| {
  let r = Registry::new();
  r.register(Box::new(queue::PENDING_MESSAGES.lock().unwrap().clone())).unwrap();
  r.register(Box::new(queue::CONSUMERS.lock().unwrap().clone())).unwrap();
  Mutex::new(r)
});

async fn respond(req: Request<Body>) -> Result<Response<Body>> {
  println!("{} {}",req.method(),req.uri());
  let encoder = TextEncoder::new();
  let metric_families;
  {
    let r = PROMETHEUS_REGISTRY.lock().unwrap();
    println!("gather metrics");
    metric_families = r.gather();
  }
  let mut buffer = Vec::<u8>::new();
  println!("encode response");
  encoder.encode(&metric_families, &mut buffer).unwrap();
  let str = String::from_utf8(buffer.clone()).unwrap();
  println!("{}",str);
  let response = Response::builder()
      .status(StatusCode::OK)
      .header(CONTENT_TYPE, encoder.format_type())
      .body(Body::from(str))
      .unwrap();
  Ok(response)
}

#[tokio::main]
async fn main() -> Result<()>  {
    println!("starting tibco-ems-operator");
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
    println!("Listening on http://{}", addr);
    if let Err(e) = server.await {
        println!("server error: {}", e);
    }

    std::thread::park();
    println!("done");
    Ok(())
}