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

pub struct StateTrigger {
  pub destination_name: String,
  pub outgoing_total_count: i64,
  pub pending_messages: i64,
}
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
  max_scale: u32,
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
        trigger_map.insert(trigger.destination_name, trigger.outgoing_total_count);
        match scale_after {
          Ok(_) => {
            State::Active(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map,
                deployment: val.deployment,
                replicas: 1,
                threshold: val.threshold,
                max_scale: val.max_scale,
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
                max_scale: val.max_scale,
              })
          },
        }
      },
      State::Active(val) => {
        let mut trigger_map = val.trigger.clone();
        let ts = get_epoch_seconds();
        trigger_map.insert(trigger.destination_name,trigger.outgoing_total_count);
        // check for scale to many
        if trigger.pending_messages > val.threshold {
          //determine scale target
          let mut scale_to = (trigger.pending_messages / val.threshold) as u32;
          if scale_to > val.max_scale {
            scale_to = val.max_scale;
          }
          if scale_to > val.replicas {
            info!("scaling up {} {}->{} replicas", val.deployment, val.replicas, scale_to);
            let scale_after = scale_to_target(&val.deployment,scale_to).await;
            match scale_after {
              Ok(_) => {
                State::Active(
                  StateValue{
                    activity_timestamp: ts,
                    trigger: trigger_map,
                    deployment: val.deployment,
                    replicas: scale_to,
                    threshold: val.threshold,
                    max_scale: val.max_scale,
                  })
              },
              Err(err) => {
                error!("scale up failed: {:?}",err);
                State::Active(
                  StateValue{
                    activity_timestamp: ts,
                    trigger: trigger_map,
                    deployment: val.deployment,
                    replicas: val.replicas,
                    threshold: val.threshold,
                    max_scale: val.max_scale,
                  })
              },
            }
          }else{
            State::Active(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map,
                deployment: val.deployment,
                replicas: val.replicas,
                threshold: val.threshold,
                max_scale: val.max_scale,
              })
          }
        } else{
          State::Active(
            StateValue{
              activity_timestamp: ts,
              trigger: trigger_map,
              deployment: val.deployment,
              replicas: val.replicas,
              threshold: val.threshold,
              max_scale: val.max_scale,
            })
        }
      },
    }
  }
  pub async fn scale_down(self, trigger: StateTrigger) -> State {
    match self{
      State::Inactive(val) => State::Inactive(val),
      State::Active(val) => {
        let deployment_name = val.deployment.clone();
        let ts = get_epoch_seconds();
        let trigger_name = trigger.destination_name.clone();
        let trigger_value = trigger.outgoing_total_count;
        let mut trigger_map = val.trigger.clone();
        let trigger_map_reader = val.trigger.clone();
        let default_value: i64 = 0;
        let old_out_total = trigger_map_reader.get(&trigger_name).unwrap_or(&default_value);
        trigger_map.insert(trigger_name,trigger_value);
        if old_out_total < &trigger_value {
          debug!("{}: still processing message while scale_down() was called",trigger.destination_name);
          trigger_map.insert(trigger.destination_name.clone(),trigger.outgoing_total_count);
          return State::Active(StateValue{
            activity_timestamp: ts,
            trigger: trigger_map.clone(),
            deployment: val.deployment,
            replicas: val.replicas,
            threshold: val.threshold,
            max_scale: val.max_scale,
          });
        }
        if val.activity_timestamp + COOLDOWN_PERIOD_SECONDS > ts {
          //honor cooldown phase
          debug!("{}: still in cooldown phase",trigger.destination_name);
          return State::Active(val);
        }
        info!("scaling down {deployment_name}");
        let scale_after = scale_to_target(&deployment_name,0).await;
        match scale_after {
          Ok(_) => State::Inactive(
              StateValue{
                activity_timestamp: ts,
                trigger: trigger_map.clone(),
                deployment: val.deployment,
                replicas: 0,
                threshold: val.threshold,
                max_scale: val.max_scale,
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
  let mut lp = ListParams::default()
    .labels("tibcoems.apimeister.com/scaling=true");

  let responsible_for = super::RESPONSIBLE_FOR.lock().unwrap().clone();
  if !responsible_for.is_empty() {
    info!("scaling Deployments for instance {responsible_for}");
    lp = lp.labels(format!("tibcoems.apimeister.com/owner={responsible_for}").as_str());
  } else {
    lp = lp.labels("!tibcoems.apimeister.com/owner");
    info!("scaling Deployments without label: tibcoems.apimeister.com/owner ");
  }

  let mut interval = time::interval(Duration::from_millis(12000));
  interval.tick().await;

  loop{
    interval.tick().await;
    for deployment in deployments.list(&lp).await.unwrap() {
      let deployment_name = ResourceExt::name_any(&deployment).clone();
      //acquire shared objects
      let mut known_scalings = KNOWN_STATES.lock().unwrap();
      let mut scale_targets = SCALE_TARGETS.lock().unwrap();
      // check if we already know about this deployment
      if known_scalings.contains_key(&deployment_name) {
        let state = known_scalings.get(&deployment_name).unwrap();
        //check replica count and create new state object
        let replica_count = deployment.spec.unwrap().replicas.unwrap();
        let deployment_state: State = match (state,replica_count) {
          (State::Active(val), 0) => {
            State::Inactive(StateValue{
              activity_timestamp: get_epoch_seconds(),
              trigger:  val.trigger.clone(),
              deployment: deployment_name.clone(),
              replicas: 0,
              threshold: val.threshold,
              max_scale: val.max_scale,
            })
          },
          (State::Inactive(val), 1) => {
            State::Active(StateValue{
              activity_timestamp: get_epoch_seconds(),
              trigger:  val.trigger.clone(),
              deployment: deployment_name.clone(),
              replicas: 1,
              threshold: val.threshold,
              max_scale: val.max_scale,
            })
          },
          _ => {
            state.clone()
          }
        };
        known_scalings.insert(deployment_name,deployment_state);
      }else{
        debug!("Found Deployment: {}", deployment_name);
        //get scale target trigger
        let d_name = deployment_name.clone();
        let mut trigger_map = StateTriggerMap::new();
        let mut labels = deployment.metadata.labels.unwrap();
        if let Some(mut annotations) = deployment.metadata.annotations {
          labels.append(&mut annotations);
        }
        let mut threshold = 100i64;
        let mut max_scale = 10u32;
        for (key,val) in labels {
          if key.starts_with("tibcoems.apimeister.com/queue") {
            //check queues on EMS Server
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
              warn!("queue cannot be monitored, because it does not exist on EMS: {queue_name}");
            }
            //add to trigger map
            trigger_map.insert(d_name.clone(),0);
          }else if key.starts_with("tibcoems.apimeister.com/threshold") {
            threshold = match val.parse::<i64>() {
              Ok(result) => result,
              Err(_err) => 100
            };
          }else if key.starts_with("tibcoems.apimeister.com/maxScale") {
            max_scale = match val.parse::<u32>() {
              Ok(result) => result,
              Err(_err) => 10
            };
          }
        }
        //check replica count and create new state object
        
        let replica_count = deployment.spec.unwrap().replicas.unwrap();
        let deployment_state: State = if replica_count == 0 {
          State::Inactive(StateValue{
            activity_timestamp: get_epoch_seconds(),
            trigger:  trigger_map.clone(),
            deployment: deployment_name.clone(),
            replicas: 0,
            threshold,
            max_scale,
          })
        }else{
          State::Active(StateValue{
            activity_timestamp: get_epoch_seconds(),
            trigger: trigger_map.clone(),
            deployment: deployment_name.clone(),
            replicas: 1,
            threshold,
            max_scale,
          })
        };
        if !trigger_map.is_empty() {
          known_scalings.insert(deployment_name, deployment_state);
        }
      }
    }
  }
}
