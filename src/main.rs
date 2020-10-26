use tokio::task;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Result, Server, StatusCode};
use hyper::header::CONTENT_TYPE;
use std::env;
use basic_tcp_proxy::TcpProxy;

mod ems;
mod queue;
mod topic;
mod bridge;

async fn respond(_req: Request<Body>) -> Result<Response<Body>> {
  // println!("{} {}",req.method(),req.uri());
  let mut body = "".to_owned();
  body.push_str("# TYPE Q:pendingMessages gauge\n");
  // body.push_str("# TYPE Q:⏳✉️ gauge\n");
  body.push_str("# TYPE Q:consumers gauge\n");
  body.push_str("# TYPE T:pendingMessages gauge\n");
  // body.push_str("# TYPE T:⏳✉️ gauge\n");
  body.push_str("# TYPE T:subscribers gauge\n");
  body.push_str("# TYPE T:durables gauge\n");
  //get queues 
  {
    let c_map = queue::QUEUES.lock().unwrap();
    for key in c_map.keys() {
      let qinfo = c_map.get(key).unwrap();
      let pending = format!("Q:pendingMessages{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.queue_name,qinfo.pending_messages);
      // let pending2 = format!("Q:⏳✉️{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.queue_name,qinfo.pending_messages);
      let consumers = format!("Q:consumers{{queue=\"{}\" instance=\"EMS-ESB\"}} {}\n",qinfo.queue_name,qinfo.consumers);
      body.push_str(&pending);
      // body.push_str(&pending2);
      body.push_str(&consumers);
    }
  }
  //get topics 
  {
    let c_map = topic::TOPICS.lock().unwrap();
    for key in c_map.keys() {
      let tinfo = c_map.get(key).unwrap();
      let pending = format!("T:pendingMessages{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.topic_name,tinfo.pending_messages);
      // let pending2 = format!("T:⏳✉️{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.topic_name,tinfo.pending_messages);
      let subscribers = format!("T:subscribers{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.durables,tinfo.subscribers);
      let durables = format!("T:durables{{topic=\"{}\" instance=\"EMS-ESB\"}} {}\n",tinfo.durables,tinfo.durables);
      body.push_str(&pending);
      // body.push_str(&pending2);
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

async fn proxy_ems(port: u16, url: String){
  println!("forwarding port: {} to {}",port,url);
  let _proxy = TcpProxy::new(port, url.parse().unwrap());
  loop {}
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

    //proxy ems connection
    let server_url = env::var("SERVER_URL").unwrap();
    if server_url.contains(",") {
      //load balanced config
      let urls: Vec<&str> = server_url.split_terminator(',').collect();
      let url1 = &urls[0].get(6..).unwrap();
      let url2 = &urls[1].get(6..).unwrap();
      let _ignore = task::spawn(proxy_ems(7222, url1.to_string()));
      let _ignore = task::spawn(proxy_ems(7223, url2.to_string()));
    }else{
      let url = server_url.get(6..).unwrap();
      let _ignore = task::spawn(proxy_ems(7222, url.to_owned()));
      let _ignore = task::spawn(proxy_ems(7223, url.to_owned()));
    }

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