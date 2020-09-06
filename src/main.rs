#[macro_use]
extern crate serde_derive;

use kube::{
    api::{Informer, Object, RawApi, Void, WatchEvent},
    client::APIClient,
    config,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Queue {
    pub name: String
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Topic {
    pub name: String
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bridge {
    pub name: String
}

// This is a convenience alias that describes the object we get from Kubernetes
type KubeQueue = Object<Queue, Void>;

fn main() {
    // Load the kubeconfig file.
    let kubeconfig = config::load_kube_config().expect("kubeconfig failed to load");

    // Create a new client
    let client = APIClient::new(kubeconfig);

    // Set a namespace. We're just hard-coding for now.
    let namespace = "default";

    // Describe the CRD we're working with.
    // This is basically the fields from our CRD definition.
    let resource = RawApi::customResource("queues")
        .group("tibcoems.apimeister.com")
        .within(&namespace);

    // Create our informer and start listening.
    let informer = Informer::raw(client, resource)
        .init()
        .expect("informer init failed");
    loop {
        informer.poll().expect("informer poll failed");

        // Now we just do something each time a new book event is triggered.
        while let Some(event) = informer.pop() {
            handle(event);
        }
    }
}

fn handle(event: WatchEvent<KubeQueue>) {
    match event {
        WatchEvent::Added(queue) => println!(
            "Added a Queue {} with name '{}'",
            queue.metadata.name, queue.spec.name
        ),
        WatchEvent::Deleted(queue) => println!("Deleted a queue {}", queue.metadata.name),
        WatchEvent::Error(e) => println!("Error {}", e),
        WatchEvent::Modified(queue) => println!("Modified Queue {}", queue.metadata.name)
    }
}
