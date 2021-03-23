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

