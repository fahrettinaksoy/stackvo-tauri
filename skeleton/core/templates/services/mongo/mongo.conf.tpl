###################################################################
# STACKVO MONGODB CONFIG TEMPLATE
###################################################################

storage:
dbPath: /data/db
journal:
enabled: true

wiredTiger:
engineConfig:
cacheSizeGB: 1

net:
port: 27017
bindIp: 0.0.0.0

security:
authorization: "enabled"

operationProfiling:
slowOpThresholdMs: 200
mode: slowOp

replication:
oplogSizeMB: 1024

setParameter:
enableLocalhostAuthBypass: false

systemLog:
verbosity: 1
destination: file
path: "/var/log/mongodb/mongod.log"
logAppend: true
