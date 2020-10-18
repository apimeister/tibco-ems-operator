use tokio::task;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Result, Server, StatusCode};

mod queue;
mod topic;
mod bridge;

async fn respond(req: Request<Body>) -> Result<Response<Body>> {
  let uri = format!("{}",req.uri().path());
  let uri_parts: Vec<&str> = uri.split('/').collect();
  let mut uri_prefix: String = uri.to_owned();
  if uri_parts.len() ==4 {
    uri_prefix = format!("/{}/{}/{}/",uri_parts[1],uri_parts[2],uri_parts[3]);
  }
  if uri_parts.len() >4 {
    uri_prefix = format!("/{}/{}/{}/{}/",uri_parts[1],uri_parts[2],uri_parts[3],uri_parts[4]);
  }
  match uri_prefix.as_str() {
    "/apis/custom.metrics.k8s.io/v1beta1/" => {
      let json_response = r#"[
        { "GroupResource":
          {
            "Group": "tibcoems.apimeister.com",
            "Resource": "Queue"
          }, 
          "Metric": "pendingMessages",
          "Namespaced": true
        }, {
          "GroupResource":
          {
            "Group": "tibcoems.apimeister.com",
            "Resource": "Queue"
          },
          "Metric": "consumerCount",
          "Namespaced":true
        }, {
          "GroupResource":
          {
            "Group": "tibcoems.apimeister.com",
            "Resource": "Topic"
          },
          "Metric": "pendingMessages",
          "Namespaced": true
        }, {
          "GroupResource":
          {
            "Group": "tibcoems.apimeister.com",
            "Resource": "Topic"
          },
          "Metric": "consumerCount",
          "Namespaced":true
        }
      ]"#;
      Ok(Response::builder()
      .status(StatusCode::OK)
      .body(Body::from(json_response))
      .unwrap())
    },
    "/apis/custom.metrics.k8s.io/v1beta1/namespaces/" => {
      //apis/custom.metrics.k8s.io/v1beta1/namespaces/default/queues.tibcoems.apimeister.com/q.test.2/pendingMessages
      let _namespace = uri_parts[5];
      let group = uri_parts[6];
      let destination = uri_parts[7];
      let metric = uri_parts[8];
      println!("group: {}, destination: {}, metric: {}",group,destination,metric);
      let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
      let ts: String = now.to_rfc3339().into();
      let mut value: i64 = 0;
      {
        let res = queue::KNOWN_QUEUES.lock().unwrap();
        match res.get(destination) {
          Some(queue) =>{
            value = queue.status.clone().unwrap().pendingMessages;
          }
          None => {},
        };
      }
      let result = json::object!{
        "kind": "MetricValueList",
        "apiVersion": "custom.metrics.k8s.io/v1beta1",
        "metadata": {
          "selfLink": json::from(uri.to_owned())
        },
        "items": [
          {
            "describedObject": {
              "kind": "Queue",
              "namespace": "default",
              "name": json::from(destination.to_owned()),
              "apiVersion": "tibcoems.apimeister.com/v1"
            },
            "metricName": "pendingMessages",
            "timestamp": json::from(ts.to_owned()),
            "value": json::from(value),
            "selector": null
          }
        ]
      };
      let result_string = result.dump();
      Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(result_string))
        .unwrap())
    },
    "/openapi/v2" => {
      Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .unwrap())
    }
    _ => {
      println!("{}: {}",&req.method(),req.uri());
      println!("prefix {}",uri_prefix);
      Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .unwrap())
    }
  }
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
    let addr = "0.0.0.0:6443".parse().unwrap();
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