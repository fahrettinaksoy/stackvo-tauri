image: "{{ image }}"
container_name: "{{ instance.container }}"
restart: unless-stopped
ports:
  - "{{ port.main }}:9092"
  - "{{ port.external }}:29092"
environment:
  KAFKA_BROKER_ID: 1
  KAFKA_ZOOKEEPER_CONNECT: "{{ companion.zookeeper.host }}:2181"
  KAFKA_LISTENERS: PLAINTEXT://0.0.0.0:9092,PLAINTEXT_HOST://0.0.0.0:29092
  KAFKA_ADVERTISED_LISTENERS: "PLAINTEXT://{{ instance.container }}:9092,PLAINTEXT_HOST://localhost:29092"
  KAFKA_LISTENER_SECURITY_PROTOCOL_MAP: PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT
  KAFKA_INTER_BROKER_LISTENER_NAME: PLAINTEXT
  KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1
  KAFKA_PROCESS_ROLES: ""
volumes:
  - "{{ volume.data }}:/var/lib/kafka/data"
networks:
  {{ network }}:
    aliases: {{ instance.aliases }}
