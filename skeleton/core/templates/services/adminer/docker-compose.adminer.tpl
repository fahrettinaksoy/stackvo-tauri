###################################################################
# STACKVO ADMINER COMPOSE TEMPLATE
###################################################################

services:
  adminer:
    profiles: ["services", "adminer"]  # --services for all, --profile adminer for this service only
    image: "adminer:{{ SERVICE_ADMINER_VERSION }}"
    container_name: "stackvo-adminer"
    restart: unless-stopped

    environment:
      ADMINER_DEFAULT_SERVER: "{{ SERVICE_ADMINER_DEFAULT_SERVER | default('stackvo-mysql') }}"
      ADMINER_DESIGN: "{{ SERVICE_ADMINER_DESIGN | default('pepa-linha') }}"

    volumes:
      - ${HOST_STACKVO_ROOT}/logs/services/adminer:/var/log/adminer

    ports:
      - "{{ SERVICE_ADMINER_HOST_PORT | default('8082') }}:8080"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.adminer.rule=Host(`{{ SERVICE_ADMINER_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.adminer.entrypoints=websecure"
      - "traefik.http.routers.adminer.tls=true"
      - "traefik.http.services.adminer.loadbalancer.server.port=8080"
