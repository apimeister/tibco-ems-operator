use kube::{api::{Api, ListParams, Resource}, Client};
use kube::Service;
use kube::config::Config;
use kube::api::PatchParams;
use core::convert::TryFrom;
use k8s_openapi::api::apps::v1::Deployment;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tokio::time::{self, Duration};
use machine::machine;
use machine::transitions;
use std::time::SystemTime;

const COOLDOWN_PERIOD_SECONDS: u64 = 60;

machine!(
  #[derive(Clone,Debug,PartialEq)]
  enum Scaling {
    Active { activity_timestamp: u64, deployment: Deployment },
    Inactive { activity_timestamp: u64, deployment: Deployment },
  }
);

#[derive(Clone,Debug,PartialEq)]
pub struct ScaleUp{
  pub activity_timestamp: u64,
}
#[derive(Clone,Debug,PartialEq)]
pub struct ScaleDown{
  pub activity_timestamp: u64,
}

transitions!(Scaling,
  [
    (Active, ScaleUp) => Active,
    (Active, ScaleDown) => [Active, Inactive],
    (Inactive, ScaleUp) => Active,
    (Inactive, ScaleDown) => Inactive
  ]
);

impl Active {
  pub fn on_scale_down(&self, event: ScaleDown) -> Scaling {
    //check for cooldown period
    if self.activity_timestamp < event.activity_timestamp - COOLDOWN_PERIOD_SECONDS {
      //scaling down
      info!("scaling down {}",self.deployment.metadata.name.clone().unwrap());
      return Scaling::Inactive(Inactive{
        activity_timestamp: event.activity_timestamp,
        deployment: self.deployment.clone(),
      })
    }else{
      //still in cooldown phase
      info!("still in cooldown phase {}",self.deployment.metadata.name.clone().unwrap());
      return Scaling::Active(Active{
        activity_timestamp: self.activity_timestamp,
        deployment: self.deployment.clone(),
      });
    }
  }
  pub fn on_scale_up(&self, event: ScaleUp ) -> Active {
    //noop
    Active{
      activity_timestamp: event.activity_timestamp,
      deployment: self.deployment.clone(),
    }
  }
}
impl Inactive {
  pub fn on_scale_down(&self, event: ScaleDown) -> Inactive {
    //noop
    Inactive{
      activity_timestamp: event.activity_timestamp,
      deployment: self.deployment.clone(),
    }
  }
  pub fn on_scale_up(&self, event: ScaleUp ) -> Active {
    let deployment_name = self.deployment.metadata.name.clone().unwrap();
    info!("scaling up {}",deployment_name);
    let patch = serde_json::json!({
        "spec": {
            "replicas": 1
        }
    });
    let params = PatchParams::apply(&deployment_name);
    let patch = kube::api::Patch::Apply(&patch);
    {
      let config = Config::from_cluster_env().unwrap();
      let service = Service::try_from(config).unwrap();
      let client: kube::Client = Client::new(service);
      let namespace = env_var!(required "KUBERNETES_NAMESPACE");
      let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
      let _ignore = deployments.patch_scale(&deployment_name,&params,&patch);
    }
    Active{
      activity_timestamp: event.activity_timestamp,
      deployment: self.deployment.clone(),
    }
  }
}

pub static KNOWN_SCALINGS: Lazy<Mutex<HashMap<String,Scaling>>> = Lazy::new(|| Mutex::new(HashMap::new()) );
pub static SCALE_TARGETS: Lazy<Mutex<HashMap<String,String>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

pub fn get_epoch_seconds() -> u64 {
  SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
}

pub async fn run(){
  let config = Config::infer().await.unwrap();
  let service = Service::try_from(config).unwrap();
  let client: kube::Client = Client::new(service);
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
  let lp = ListParams::default()
    .labels("tibcoems.apimeister.com/scaling=true");
  let mut interval = time::interval(Duration::from_millis(60000));
  interval.tick().await;

  loop{
    interval.tick().await;
    for deployment in deployments.list(&lp).await.unwrap() {
      let deployment_name = Resource::name(&deployment).clone();
      //acquire shared objects
      let mut known_scalings = KNOWN_SCALINGS.lock().unwrap();
      let mut scale_targets = SCALE_TARGETS.lock().unwrap();
      if !known_scalings.contains_key(&deployment_name) {
        info!("Found Deployment: {}", deployment_name);
        let scaling = Scaling::Inactive(
          Inactive{
            activity_timestamp: get_epoch_seconds(),
            deployment: deployment.clone(),
          }
        );
        let mut has_queues = false;
        let d_name = deployment_name.clone();
        let labels = deployment.metadata.labels.unwrap();
        for (key,val) in labels {
          if key.starts_with("tibcoems.apimeister.com/queue") {
            //check known queues
            let all_queues = super::queue::QUEUES.lock().unwrap();
            if all_queues.contains_key(&val) {
              //known queue
              info!("add queue scaler queue: {}, deployment: {}",val,d_name);
              scale_targets.insert(val,d_name.clone());
              has_queues = true;
            }else{
              //queue does not exist
              warn!("queue cannot be monitored, because it does not exists: {}",val);
            }
          }
        }
        if has_queues {
          known_scalings.insert(deployment_name,scaling);
        }
      }
    //   let patch = serde_json::json!({
    //     "apiVersion": "v1",
    //     "kind": "Pod",
    //     "metadata": {
    //         "name": "blog"
    //     },
    //     "spec": {
    //         "activeDeadlineSeconds": 5
    //     }
    // });
    // //$sys.monitor.q.r.test.q
    // let params = PatchParams::apply("myapp");
    // let patch = Patch::Apply(&patch);
    //   deployments.patch_scale("name",pp,patch);
    }
  }
}