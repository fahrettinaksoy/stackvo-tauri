###################################################################
# STACKVO MONGO COMPOSE TEMPLATE
###################################################################

services:
  mongo:
    profiles: ["services", "mongo"]  # --services for all, --profile mongo for this service only
    image: "mongo:{{ SERVICE_MONGO_VERSION }}"
    container_name: "stackvo-mongo"
    restart: unless-stopped

    # Single-node replica set, not standalone. Change streams (db.watch()) and
    # retryable writes need an oplog, which only a replica set has; a standalone
    # mongod rejects $changeStream with "only supported on replica sets".
    #
    # --replSet together with --auth forces internal cluster auth, so mongod
    # refuses to boot without a keyFile ("security.keyFile is required when
    # authorization is enabled with replica sets"). The keyFile is generated on
    # first boot inside the data volume, so it survives restarts and never has to
    # be bind-mounted from the host (a host bind mount would carry the host's uid
    # and mode, and mongod rejects a keyFile it does not own or that is group/world
    # readable).
    #
    # The member is registered as "stackvo-mongo:27017" so other containers can
    # follow replica-set discovery. A client on the HOST talking to 127.0.0.1:27017
    # discovers that name, cannot resolve it, and hangs — host tools (Compass,
    # TablePlus) must append "directConnection=true" to the connection string.
    command:
      - "bash"
      - "-c"
      - |
        if [ ! -s /data/db/.keyfile ]; then openssl rand -base64 756 > /data/db/.keyfile; fi
        chmod 400 /data/db/.keyfile
        chown mongodb:mongodb /data/db/.keyfile
        (
          until mongosh --quiet -u "{{ SERVICE_MONGO_INITDB_ROOT_USERNAME | default('root') }}" -p "{{ SERVICE_MONGO_INITDB_ROOT_PASSWORD | default('root') }}" --authenticationDatabase admin --eval "db.adminCommand({ping:1})" >/dev/null 2>&1; do sleep 1; done
          mongosh --quiet -u "{{ SERVICE_MONGO_INITDB_ROOT_USERNAME | default('root') }}" -p "{{ SERVICE_MONGO_INITDB_ROOT_PASSWORD | default('root') }}" --authenticationDatabase admin --eval "try { rs.status() } catch (e) { rs.initiate({_id: \"{{ SERVICE_MONGO_REPLSET | default('rs0') }}\", members: [{_id: 0, host: \"stackvo-mongo:27017\"}]}) }" >/dev/null 2>&1
        ) &
        exec docker-entrypoint.sh mongod --auth --bind_ip_all --replSet {{ SERVICE_MONGO_REPLSET | default('rs0') }} --keyFile /data/db/.keyfile

    environment:
      MONGO_INITDB_ROOT_USERNAME: "{{ SERVICE_MONGO_INITDB_ROOT_USERNAME | default('root') }}"
      MONGO_INITDB_ROOT_PASSWORD: "{{ SERVICE_MONGO_INITDB_ROOT_PASSWORD | default('root') }}"
      MONGO_INITDB_DATABASE: "{{ SERVICE_MONGO_DATABASE | default('stackvo') }}"

    volumes:
      - stackvo-mongo-data:/data/db
      - ${HOST_STACKVO_ROOT}/generated/configs/mongo.conf:/etc/mongo/mongo.conf:ro
      - ${HOST_STACKVO_ROOT}/logs/services/mongo:/var/log/mongodb

    ports:
      - "{{ HOST_PORT_MONGO | default('27017') }}:27017"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-mongo-data:
