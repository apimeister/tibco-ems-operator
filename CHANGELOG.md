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
