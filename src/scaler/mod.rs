use kube::{api::{Api, ListParams, ResourceExt, Patch}, Client};
use kube::api::PatchParams;
use kube::core::subresource::Scale;
use kube::Error;
use k8s_openapi::api::apps::v1::Deployment;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tokio::time::{self, Duration};
use std::time::SystemTime;

/// period to wait before a scale down can be performed
const COOLDOWN_PERIOD_SECONDS: u64 = 60;

/// StateTrigger with 0 being the destination name, 1 being the pending messages
pub type StateTrigger = (String, i64);
/// TriggerMap contains of string (queue name) and i64 (outbound_message_count)
type StateTriggerMap = HashMap<String, i64>;

/// HashMap of Deployment Name with the value of Deployment State
pub static KNOWN_STATES: Lazy<Mutex<HashMap<String,State>>> = Lazy::new(|| Mutex::new(HashMap::new()) );
/// HashMap of Queue Name with the value as Vector of deployment names
pub static SCALE_TARGETS: Lazy<Mutex<HashMap<String,Vec<String>>>> = Lazy::new(|| Mutex::new(HashMap::new()) );

/// Represents the state of the k8s Deployment
#[derive(Clone,Debug,PartialEq)]
pub struct StateValue{
  /// timestamp of the last update
  activity_timestamp: u64,
  /// map of all triggering queues
  trigger: StateTriggerMap,
  /// name of the k8s deployment
  deployment: String,
  /// number of replicas
  replicas: u32,
  /// threshold for scaling
  /// scaling is happening lineary, e.g. threshold 100 leads to the following behavoir:
  /// 0 messages pending -> zero replicas
  /// 1 message pending -> 1 replica
  /// 100 message pending -> 2 replicas
  /// 1000 messages pending -> 10 replicas
  threshold: i64,
}
/// Represents the state of the k8s Deployment
#[derive(Clone,Debug,PartialEq)]
pub enum State{
  /// deployment is scaled to 0 replicas
  Inactive(StateValue),
  /// deployment is scaled to at least 1 replica
  Active(StateValue),
}

impl State {
  pub async fn scale_up(self, trigger: StateTrigger) -> State {
    match self{
      State::Inactive(val) => {
        let deployment_name = val.deployment.clone();
        info!("scaling up {}",deployment_name);
        let ts = get_epoch_seconds();
        let scale_after = scale_to_target(&deployment_name,1).await;
        let mut trigger_map = val.trigger.clone();
        trigger_map.insert(trigger.0, trigger.1);
        match scale_after {
          Ok(_) => {
            State::Active(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map,
                deployment: val.deployment,
                replicas: 1,
                threshold: val.threshold,
              })    
          },
          Err(err) => {
            error!("scale up failed: {:?}",err);
            State::Inactive(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map,
                deployment: val.deployment,
                replicas: 0,
                threshold: val.threshold,
              })
          },
        }
      },
      State::Active(val) => {
        let mut trigger_map = val.trigger.clone();
        let ts = get_epoch_seconds();
        trigger_map.insert(trigger.0,trigger.1);
        // check for scale to many
        if trigger.1 > val.threshold {

        }
        State::Active(
          StateValue{
            activity_timestamp: ts,
            trigger: trigger_map,
            deployment: val.deployment,
            replicas: 1,
            threshold: val.threshold,
          })
      },
    }
  }
  pub async fn scale_down(self, trigger: StateTrigger) -> State {
    match self{
      State::Inactive(val) => State::Inactive(val),
      State::Active(val) => {
        let deployment_name = val.deployment.clone();
        let ts = get_epoch_seconds();
        let trigger_name = trigger.0.clone();
        let trigger_value = trigger.1;
        let mut trigger_map = val.trigger.clone();
        let trigger_map_reader = val.trigger.clone();
        let default_value: i64 = 0;
        let old_out_total = trigger_map_reader.get(&trigger_name).or(Some(&default_value)).unwrap();
        trigger_map.insert(trigger_name,trigger_value);
        if old_out_total < &trigger_value {
          debug!("{}: still processing message while scale_down() was called",trigger.0);
          trigger_map.insert(trigger.0.clone(),trigger.1);
          return State::Active(StateValue{
            activity_timestamp: ts,
            trigger: trigger_map.clone(),
            deployment: val.deployment,
            replicas: 1,
            threshold: val.threshold,
          });
        }
        if val.activity_timestamp + COOLDOWN_PERIOD_SECONDS > ts {
          //honor cooldown phase
          debug!("{}: still in cooldown phase",trigger.0);
          return State::Active(val);
        }
        info!("scaling down {}",deployment_name);
        let scale_after = scale_to_target(&deployment_name,0).await;
        match scale_after {
          Ok(_) => State::Inactive(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map.clone(),
                deployment: val.deployment,
                replicas: 0,
                threshold: val.threshold,
              }),
          Err(err) => {
            error!("scale down failed: {}",err);
            State::Active(val)
          }
        }
      },
    }
  }
}

async fn scale_to_target(deployment_name: &str, replicas: u32) -> Result<Scale, Error>  {
  let client = Client::try_default().await.expect("getting default client");
  let namespace = env_var!(required "KUBERNETES_NAMESPACE");
  let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
  let scale_spec = serde_json::json!({
    "spec": { "replicas": replicas }
  });
  let patch_params = PatchParams::default();
  deployments.patch_scale(deployment_name, &patch_params, &Patch::Merge(&scale_spec)).await
}

pub fn get_epoch_seconds() -> u64 {
  SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
}

/// watches for k8s Deployments with scaling labels present
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
              replicas: 0,
              threshold: val.threshold,
            });
          },
          (State::Inactive(val), 1) => {
            deployment_state = State::Active(StateValue{
              activity_timestamp: get_epoch_seconds(),
              trigger:  val.trigger.clone(),
              deployment: deployment_name.clone(),
              replicas: 1,
              threshold: val.threshold,
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
        let labels = deployment.metadata.labels.unwrap();
        let mut threshold = 100i64;
        for (key,val) in labels {
          if key.starts_with("tibcoems.apimeister.com/queue") {
            //check known queues
            let queue_name = val;
            let all_queues = super::queue::QUEUES.lock().unwrap();
            if all_queues.contains_key(&queue_name) {
              //known queue
              if scale_targets.contains_key(&queue_name) {
                let mut x: Vec<String> =scale_targets.get(&queue_name).unwrap().clone();
                //check if the vector contains the value d_name
                if !x.contains(&d_name) {
                  x.push(d_name.clone());
                }
                info!("add queue scaler queue: {}, deployment: {:?}",queue_name,x);
                scale_targets.insert(queue_name.clone(),x);
              }else{
                info!("add queue scaler queue: {}, deployment: {}",queue_name,d_name);
                scale_targets.insert(queue_name.clone(),vec![d_name.clone()]);
              }
            }else{
              //queue does not exist
              warn!("queue cannot be monitored, because it does not exists: {}",queue_name);
            }
            //add to trigger map
            trigger_map.insert(d_name.clone(),0);
          }else if key.starts_with("tibcoems.apimeister.com/threshold") {
            threshold = match val.parse::<i64>() {
              Ok(result) => result,
              Err(_err) => 100
            };
          }
        }
        //check replica count and create new state object
        let deployment_state: State;
        let replica_count = deployment.spec.unwrap().replicas.unwrap();
        if replica_count == 0 {
          deployment_state = State::Inactive(StateValue{
            activity_timestamp: get_epoch_seconds(),
            trigger:  trigger_map.clone(),
            deployment: deployment_name.clone(),
            replicas: 0,
            threshold,
          });
        }else{
          deployment_state = State::Active(StateValue{
            activity_timestamp: get_epoch_seconds(),
            trigger: trigger_map.clone(),
            deployment: deployment_name.clone(),
            replicas: 1,
            threshold,
          });
        }
        if !trigger_map.is_empty() {
          known_scalings.insert(deployment_name, deployment_state);
        }
      }
    }
  }
}
