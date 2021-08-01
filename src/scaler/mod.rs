use kube::{api::{Api, ListParams, ResourceExt, Patch}, Client};
use kube::api::PatchParams;
use k8s_openapi::api::apps::v1::Deployment;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tokio::time::{self, Duration};
use std::time::SystemTime;

const COOLDOWN_PERIOD_SECONDS: u64 = 60;

pub type StateTrigger = (String, i64);
type StateTriggerMap = HashMap<String, i64>;

#[derive(Clone,Debug,PartialEq)]
pub struct StateValue{
  activity_timestamp: u64,
  trigger: StateTriggerMap,
  deployment: String,
}
#[derive(Clone,Debug,PartialEq)]
pub enum State{
  Inactive(StateValue),
  Active(StateValue),
}
impl State {
  pub fn new(deployment: String,trigger: StateTriggerMap) -> State {
    State::Inactive(StateValue{ 
      activity_timestamp: get_epoch_seconds(),
      trigger: trigger,
      deployment: deployment,
    })
  }
}

impl State {
  pub async fn scale_up(self, trigger: StateTrigger) -> State {
    match self{
      State::Inactive(val) => {
        let deployment_name = val.deployment.clone();
        info!("scaling up {}",deployment_name);
        let client = Client::try_default().await.expect("getting default client");
        let namespace = env_var!(required "KUBERNETES_NAMESPACE");
        let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
        let ts = get_epoch_seconds();
        let scale_spec = serde_json::json!({
          "spec": { "replicas": 1 }
        });
        let patch_params = PatchParams::default();
        let scale_after = deployments.patch_scale(&deployment_name, &patch_params, &Patch::Merge(&scale_spec)).await;
        let mut trigger_map = val.trigger.clone();
        trigger_map.insert(trigger.0, trigger.1);
        match scale_after {
          Ok(_val) => {
            return State::Active(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map,
                deployment: val.deployment,
              }
            );
    
          },
          Err(err) => {
            error!("scale up failed: {:?}",err);
            return State::Inactive(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map,
                deployment: val.deployment,
              }
            );
          },
        }
      },
      State::Active(val) => {
        let mut trigger_map = val.trigger.clone();
        let ts = get_epoch_seconds();
        trigger_map.insert(trigger.0,trigger.1);
        return State::Active(
          StateValue{
            activity_timestamp: ts,
            trigger: trigger_map,
            deployment: val.deployment,
          }
        );
      },
    }
  }
  pub async fn scale_down(self, trigger: StateTrigger) -> State {
    match self{
      State::Inactive(val) => {
        return State::Inactive(val);
      },
      State::Active(val) => {
        let deployment_name = val.deployment.clone();
        let ts = get_epoch_seconds();
        let trigger_name = trigger.0.clone();
        let trigger_value = trigger.1.clone();
        let mut trigger_map = val.trigger.clone();
        let trigger_map_reader = val.trigger.clone();
        let old_out_total = trigger_map_reader.get(&trigger_name).unwrap();
        trigger_map.insert(trigger_name,trigger_value);
        if old_out_total < &trigger_value {
          debug!("{}: stile processing message while scale_down() was called",trigger.0);
          trigger_map.insert(trigger.0.clone(),trigger.1);
          return State::Active(StateValue{
            activity_timestamp: ts,
            trigger: trigger_map.clone(),
            deployment: val.deployment,
          });
        }
        if val.activity_timestamp + COOLDOWN_PERIOD_SECONDS > ts {
          //honor cooldown phase
          debug!("{}: still in cooldown phase",trigger.0);
          return State::Active(val);
        }
        info!("scaling down {}",deployment_name);
        let client = Client::try_default().await.expect("getting default client");
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
                trigger: trigger_map.clone(),
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
  let client = Client::try_default().await.expect("getting default client");
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
      // check if we already know about this deployment
      if known_scalings.contains_key(&deployment_name) {
        let state = known_scalings.get(&deployment_name).unwrap();
        //check replica count and create new state object
        let deployment_state: State;
        let replica_count = deployment.spec.unwrap().replicas.unwrap();
        match (state,replica_count) {
          (State::Active(val), 0) => {
            deployment_state = State::Inactive(StateValue{
              activity_timestamp: get_epoch_seconds(),
              trigger:  val.trigger.clone(),
              deployment: deployment_name.clone(),
            });
          },
          (State::Inactive(val), 1) => {
            deployment_state = State::Active(StateValue{
              activity_timestamp: get_epoch_seconds(),
              trigger:  val.trigger.clone(),
              deployment: deployment_name.clone(),
            });
          },
          _ => {
            deployment_state = state.clone();
          }
        }
        known_scalings.insert(deployment_name,deployment_state);
      }else{
        info!("Found Deployment: {}", deployment_name);
        //get scale target trigger
        let d_name = deployment_name.clone();
        let mut trigger_map = StateTriggerMap::new();
        let labels = deployment.metadata.labels;
        for (key,val) in labels {
          if key.starts_with("tibcoems.apimeister.com/queue") {
            //check known queues
            let all_queues = super::queue::QUEUES.lock().unwrap();
            if all_queues.contains_key(&val) {
              //known queue
              info!("add queue scaler queue: {}, deployment: {}",val,d_name);
              scale_targets.insert(val.clone(),d_name.clone());
              trigger_map.insert(val.clone(),0);
            }else{
              //queue does not exist
              warn!("queue cannot be monitored, because it does not exists: {}",val);
            }
          }
        }
        //check replica count and create new state object
        let deployment_state: State;
        let replica_count = deployment.spec.unwrap().replicas.unwrap();
        if replica_count == 0 {
          deployment_state = State::new(deployment_name.clone(),trigger_map.clone());
        }else{
          deployment_state = State::Active(StateValue{
            activity_timestamp: get_epoch_seconds(),
            trigger: trigger_map.clone(),
            deployment: deployment_name.clone(),
          });
        }

        if trigger_map.len() > 0 {
          known_scalings.insert(deployment_name,deployment_state);
        }
      }
    }
  }
}