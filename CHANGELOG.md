# tibco-ems-operator:51/2022-06-15

* switch base image to debian-slim
* fix replica value for prevented scale_down

# tibco-ems-operator:50/2022-05-12

* refine logging for scale up
* update dependencies

# tibco-ems-operator:49/2022-03-23

* update dependencies

# tibco-ems-operator:48/2022-03-23

* update dependencies
* introduce object responsibilities (EMS instance ownership through annotation)
* fix API query path

# tibco-ems-operator:47/2022-01-20

* update dependencies
* change http impl to axum
* support annotation based scaling
* support container shutdown signal

# tibco-ems-operator:46/2021-12-09

* support scale to many (including threshold and maxScale label)
* update to Tibco EMS 10.1

# tibco-ems-operator:45/2021-09-19

* fix creation of bridges, now all briges (Q-Q,T-T,T-Q,Q-T) are created

# tibco-ems-operator:44/2021-09-14

* support unescaping of all escaped URI chars
* support prefetch for queue/topic creation

# tibco-ems-operator:43/2021-08-17

* implement clippy recommendations

# tibco-ems-operator:42/0.9.0/2021-08-04

* support multiple scaling targets for a single queue

# tibco-ems-operator:41/0.8.0/2021-08-01

* prevent scale down while engine is still consuming messages
* support for scaling with multiple queues

# tibco-ems-operator:40/0.7.0/2021-07-22

* honor initial replica value on k8s deployments
* handle create/delete queue errors
* handle create/delete topic errors
* handle create/delete bridge errors

# tibco-ems-operator:39/0.6.0/2021-07-15

* shutdown operator on panic
* switch base image to rockylinux

# tibco-ems-operator:38/0.5.0/2021-07-01

* panic on EMS disconnect 

# tibco-ems-operator:37/0.4.0/2021-06-20

* update dependencies

# tibco-ems-operator:36/0.3.0/2021-06-03

* fix expiration for queue/topic

# tibco-ems-operator:35/0.2.0/2021-05-31

* update kube to 0.55
* ubdate tibco-ems to 0.3
