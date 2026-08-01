###################################################################
# STACKVO KAFBAT KAFKA UI COMPOSE TEMPLATE
###################################################################

services:
  kafbat:
    profiles: ["services", "kafbat"]  # --services for all, --profile kafbat for this service only
    image: "kafbat/kafka-ui:{{ SERVICE_KAFBAT_VERSION }}"
    container_name: "stackvo-kafbat"
    restart: unless-stopped

    environment:
      DYNAMIC_CONFIG_ENABLED: "{{ SERVICE_KAFBAT_DYNAMIC_CONFIG | default('true') }}"
      KAFKA_CLUSTERS_0_NAME: "{{ SERVICE_KAFBAT_CLUSTER_NAME | default('stackvo-kafka') }}"
      KAFKA_CLUSTERS_0_BOOTSTRAPSERVERS: "{{ SERVICE_KAFBAT_BOOTSTRAP_SERVERS | default('stackvo-kafka:9092') }}"

    volumes:
      - ${HOST_STACKVO_ROOT}/logs/services/kafbat:/var/log/kafbat

    ports:
      - "{{ SERVICE_KAFBAT_HOST_PORT | default('8080') }}:8080"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.kafbat.rule=Host(`{{ SERVICE_KAFBAT_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.kafbat.entrypoints=websecure"
      - "traefik.http.routers.kafbat.tls=true"
      - "traefik.http.services.kafbat.loadbalancer.server.port=8080"
