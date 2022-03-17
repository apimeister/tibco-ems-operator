# tibco-ems-operator

This project is a work in progress. It is not yet functional and I will update this readme, if the operator is ready to use.

## available ENV properties:

| name |cardinality | value |  description |
| --- | --- | --- | --- |
| DO_NOT_DELETE_OBJECTS | optional | FALSE | if set to TRUE (all caps), object are not deleted from EMS |
| READ_ONLY | optional | FALSE | if set to TRUE (all caps), no objects are created, the operator only collects statistics (which are only propagated through metrics endpoint) |
|KUBERNETES_SERVICE_HOST |required | kubernetes.default.svc.cluster.local | references the api server, if not present, the rust TLS will fail because it cannot validate the IP of the API server |
|STATUS_REFRESH_IN_MS |required | 10000 | how often statistics are refreshed |
|KUBERNETES_NAMESPACE | required | {ref metadata.namespace} | what namespace should be captured |
| SERVER_URL | required | tcp://ems:7222 | |
| USERNAME | required | {user} | |
| PASSWORD | required | {password} | |
| ADMIN_COMMAND_TIMEOUT_MS | optional | 60000 | command timeout in milliseconds, default is 60000 |
| ENABLE_SCALING | optional | FALSE | if set to TRUE (all caps), deployment can be scaled through the operator |
| RESPONSIBLE_FOR | optional | {ems_instance} | if set, only objects with the owner annotation will be honoered by this operator instance |

## Scaling

The operator can be used to dynamically scale deployment from zero to one.

To enable scaling on a deployment, the following labels have to be set.

```
apiVersion: apps/v1
kind: Deployment
metadata:
  labels:
    app: sample-app
    tibcoems.apimeister.com/scaling: "true"
    tibcoems.apimeister.com/queue.1: test.q
  name: sample-app
spec:
  replicas: 0
  ...
```

#### Other Scaling Properties

| property | default | description |
|----------|---------|-------------|
| scaling  | false   | enable scaling for deployment |
| queue.*  | n/a     | destination to scale for |
| threshold | 100    | scaling threshold for scaling to more then one engine |
| maxScale  | 10     | max replicas for auto-scaling |

## Ownership

The operator can be deploymed mutliple times into one namespace, as long as the ownership of the managed objects is also explicitly defined.

### Operator


```
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tibco-ems-operator
spec:
  template:
    spec:
      containers:
      - name: tibco-ems-operator
        image: tibco-ems-operator:latest
        env:
          - name: RESPONSIBLE_FOR
            value: EMS_CENTRAL_1
```

### Managed object

```
apiVersion: tibcoems.apimeister.com/v1
kind: Queue
metadata:
  name: q.test.1
  tibcoems.apimeister.com/owner: EMS_CENTRAL_1
spec: {}
```