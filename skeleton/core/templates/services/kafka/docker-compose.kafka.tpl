###################################################################
# STACKVO KAFKA COMPOSE TEMPLATE
###################################################################

services:
  zookeeper:
    profiles: ["services", "kafka"]  # --services for all, --profile kafka for this service only
    image: confluentinc/cp-zookeeper:latest
    container_name: stackvo-zookeeper
    restart: unless-stopped
    environment:
      ZOOKEEPER_CLIENT_PORT: 2181
      ZOOKEEPER_TICK_TIME: 2000
    volumes:
      - ${HOST_STACKVO_ROOT}/logs/services/zookeeper:/var/log/zookeeper
    ports:
      - "2181:2181"
    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

  kafka:
    image: confluentinc/cp-kafka:{{ SERVICE_KAFKA_VERSION }}
    container_name: stackvo-kafka
    restart: unless-stopped
    ports:
      - "{{ SERVICE_KAFKA_HOST_PORT | default('9092') }}:9092"
      - "{{ SERVICE_KAFKA_EXTERNAL_HOST_PORT | default('29092') }}:29092"
    environment:
      KAFKA_BROKER_ID: 1
      KAFKA_ZOOKEEPER_CONNECT: zookeeper:2181
      KAFKA_LISTENERS: PLAINTEXT://0.0.0.0:9092,PLAINTEXT_HOST://0.0.0.0:29092
      KAFKA_ADVERTISED_LISTENERS: PLAINTEXT://stackvo-kafka:9092,PLAINTEXT_HOST://localhost:29092
      KAFKA_LISTENER_SECURITY_PROTOCOL_MAP: PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT
      KAFKA_INTER_BROKER_LISTENER_NAME: PLAINTEXT
      KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1
      KAFKA_PROCESS_ROLES: ""
    volumes:
      - stackvo-kafka-data:/var/lib/kafka/data
      - ${HOST_STACKVO_ROOT}/logs/services/kafka:/var/log/kafka
    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-kafka-data:
