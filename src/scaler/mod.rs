use kube::{api::{Api, ListParams, ResourceExt, Patch}, Client};
use kube::Service;
use kube::config::Config;
use kube::api::PatchParams;
use core::convert::TryFrom;
use k8s_openapi::api::apps::v1::Deployment;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tokio::time::{self, Duration};
use std::time::SystemTime;

const COOLDOWN_PERIOD_SECONDS: u64 = 60;

#[derive(Clone,Debug,PartialEq)]
pub struct StateValue{
  activity_timestamp: u64,
  outgoing_total_count: i64,
  deployment: String,
}
#[derive(Clone,Debug,PartialEq)]
pub enum State{
  Inactive(StateValue),
  Active(StateValue),
}
impl State {
  pub fn new(deployment: String) -> State {
    State::Inactive(StateValue{ 
      activity_timestamp: get_epoch_seconds(),
      outgoing_total_count: 0,
      deployment: deployment,
    })
  }
}

impl State {
  pub async fn scale_up(self, outgoing_total_count: i64) -> State {
    match self{
      State::Inactive(val) => {
        let deployment_name = val.deployment.clone();
        info!("scaling up {}",deployment_name);
        let config = Config::from_cluster_env().unwrap();
        let service = Service::try_from(config).unwrap();
        let client: kube::Client = Client::new(service);
        let namespace = env_var!(required "KUBERNETES_NAMESPACE");
        let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
        let ts = get_epoch_seconds();
        let scale_spec = serde_json::json!({
          "spec": { "replicas": 1 }
        });
        let patch_params = PatchParams::default();
        let scale_after = deployments.patch_scale(&deployment_name, &patch_params, &Patch::Merge(&scale_spec)).await;
        match scale_after {
          Ok(_val) => {
            return State::Active(
              StateValue{
                activity_timestamp: ts,
                outgoing_total_count: outgoing_total_count,
                deployment: val.deployment,
              }
            );
    
          },
          Err(err) => {
            error!("scale up failed: {:?}",err);
            return State::Inactive(
              StateValue{
                activity_timestamp: ts,
                outgoing_total_count: outgoing_total_count,
                deployment: val.deployment,
              }
            );
          },
        }
      },
      State::Active(val) => {
        return State::Active(val);
      },
    }
  }
  pub async fn scale_down(self, outgoing_total_count: i64) -> State {
    match self{
      State::Inactive(val) => {
        return State::Inactive(val);
      },
      State::Active(val) => {
        let deployment_name = val.deployment.clone();
        let ts = get_epoch_seconds();
        if val.outgoing_total_count > outgoing_total_count {
          //message processing still happening
          return State::Active(StateValue{
            activity_timestamp: ts,
            outgoing_total_count: outgoing_total_count,
            deployment: val.deployment,
          });
        }
        if val.activity_timestamp + COOLDOWN_PERIOD_SECONDS > ts {
          //honor cooldown phase
          return State::Active(val);
        }
        info!("scaling down {}",deployment_name);
        let config = Config::from_cluster_env().unwrap();
        let service = Service::try_from(config).unwrap();
        let client: kube::Client = Client::new(service);
        let namespace = env_var!(required "KUBERNETES_NAMESPACE");
        let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
        let scale_spec = serde_json::json!({
          "spec": { "replicas": 0 }
        });
        let patch_params = PatchParams::default();
        let scale_after = deployments.patch_scale(&deployment_name, &patch_params, &Patch::Merge(&scale_spec)).await;
        match scale_after {
          Ok(_state) => {
            return State::Inactive(
              StateValue{
                activity_timestamp: ts,
                outgoing_total_count: outgoing_total_count,
                deployment: val.deployment,
              }
            );
          },
          Err(err) => {
            error!("scale down failed: {}",err);
            return State::Active(val);
          }
        }
      },
    }
  }
}

pub static KNOWN_STATES: Lazy<Mutex<HashMap<String,State>>> = Lazy::new(|| Mutex::new(HashMap::new()) );
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
  let mut interval = time::interval(Duration::from_millis(12000));
  interval.tick().await;

  loop{
    interval.tick().await;
    for deployment in deployments.list(&lp).await.unwrap() {
      let deployment_name = ResourceExt::name(&deployment).clone();
      //acquire shared objects
      let mut known_scalings = KNOWN_STATES.lock().unwrap();
      let mut scale_targets = SCALE_TARGETS.lock().unwrap();
      if !known_scalings.contains_key(&deployment_name) {
        info!("Found Deployment: {}", deployment_name);
        let state_inactive = State::new(deployment_name.clone());
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
          known_scalings.insert(deployment_name,state_inactive);
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