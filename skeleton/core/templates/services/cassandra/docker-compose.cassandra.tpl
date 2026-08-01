###################################################################
# STACKVO CASSANDRA COMPOSE TEMPLATE
###################################################################

services:
  cassandra:
    profiles: ["services", "cassandra"]  # --services for all, --profile cassandra for this service only
    image: "cassandra:{{ SERVICE_CASSANDRA_VERSION }}"
    container_name: "stackvo-cassandra"
    restart: unless-stopped

    environment:
      CASSANDRA_CLUSTER_NAME: "{{ SERVICE_CASSANDRA_CLUSTER_NAME | default('StackvoCluster') }}"
      CASSANDRA_DC: "{{ SERVICE_CASSANDRA_DC | default('dc1') }}"
      CASSANDRA_RACK: "{{ SERVICE_CASSANDRA_RACK | default('rack1') }}"
      CASSANDRA_ENDPOINT_SNITCH: "{{ SERVICE_CASSANDRA_ENDPOINT_SNITCH | default('GossipingPropertyFileSnitch') }}"
      MAX_HEAP_SIZE: "{{ SERVICE_CASSANDRA_MAX_HEAP_SIZE | default('512M') }}"
      HEAP_NEWSIZE: "{{ SERVICE_CASSANDRA_HEAP_NEWSIZE | default('128M') }}"

    volumes:
      - stackvo-cassandra-data:/var/lib/cassandra
      - ${HOST_STACKVO_ROOT}/logs/services/cassandra:/var/log/cassandra

    ports:
      - "{{ HOST_PORT_CASSANDRA | default('9042') }}:9042"
      - "{{ HOST_PORT_CASSANDRA_JMX | default('7199') }}:7199"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    healthcheck:
      test: ["CMD-SHELL", "cqlsh -e 'describe cluster'"]
      interval: 30s
      timeout: 10s
      retries: 5

volumes:
  stackvo-cassandra-data:
