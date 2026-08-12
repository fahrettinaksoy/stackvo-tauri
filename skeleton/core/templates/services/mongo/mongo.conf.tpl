###################################################################
# STACKVO MONGODB CONFIG TEMPLATE
###################################################################
#
# Loaded by the compose template with `mongod --config /etc/mongo/mongo.conf`.
# Until that flag existed this file was mounted but never read, so nothing in
# it had any effect. It also could not have been read: the keys were written
# flat, and mongod rejects that with "Unrecognized option: storage".
#
# Files under templates/services are copied verbatim, not rendered, so no
# `{{ PLACEHOLDER }}` may appear here — it would reach mongod unsubstituted and
# the server would refuse to start. Anything stackvo parameterises (replica set
# name, keyFile path) therefore stays on the command line.
#
# Deliberately absent, each for a reason that costs a broken container:
#
#   net.bindIp        The command line passes --bind_ip_all. Setting both is a
#                     startup error ("Cannot specify both bind_ip_all and
#                     bind_ip").
#   security          --auth and --keyFile are on the command line, next to
#                     --replSet, because the keyFile path is generated there.
#   storage.journal   Removed in MongoDB 7.0; journalling is now always on.
#                     Leaving it in aborts 8.0 with "Unrecognized option:
#                     storage.journal.enabled".
#   systemLog         Logging stays on stdout so `docker logs` works, matching
#                     the other services (GEMINI.md compliance).

storage:
  dbPath: /data/db
  wiredTiger:
    engineConfig:
      # A dev machine runs a dozen containers at once. WiredTiger's default is
      # half of host RAM, which starves everything else on the box.
      cacheSizeGB: 1

operationProfiling:
  mode: slowOp
  slowOpThresholdMs: 200

replication:
  # replSetName is on the command line (stackvo parameterises it).
  oplogSizeMB: 1024

setParameter:
  enableLocalhostAuthBypass: false
