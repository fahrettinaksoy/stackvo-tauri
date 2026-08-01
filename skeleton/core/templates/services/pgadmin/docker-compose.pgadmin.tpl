###################################################################
# STACKVO PGADMIN4 COMPOSE TEMPLATE
###################################################################

services:
  pgadmin:
    profiles: ["services", "pgadmin"]  # --services for all, --profile pgadmin for this service only
    image: "dpage/pgadmin4:{{ SERVICE_PGADMIN_VERSION }}"
    container_name: "stackvo-pgadmin"
    restart: unless-stopped

    environment:
      PGADMIN_DEFAULT_EMAIL: "{{ SERVICE_PGADMIN_DEFAULT_EMAIL | default('admin@stackvo.loc') }}"
      PGADMIN_DEFAULT_PASSWORD: "{{ SERVICE_PGADMIN_DEFAULT_PASSWORD | default('admin') }}"
      PGADMIN_CONFIG_SERVER_MODE: "{{ SERVICE_PGADMIN_SERVER_MODE | default('False') }}"
      PGADMIN_CONFIG_MASTER_PASSWORD_REQUIRED: "{{ SERVICE_PGADMIN_MASTER_PASSWORD_REQUIRED | default('False') }}"

    volumes:
      - stackvo-pgadmin-data:/var/lib/pgadmin
      - ${HOST_STACKVO_ROOT}/logs/services/pgadmin:/var/log/pgadmin

    ports:
      - "{{ SERVICE_PGADMIN_HOST_PORT | default('5050') }}:80"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.pgadmin.rule=Host(`{{ SERVICE_PGADMIN_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.pgadmin.entrypoints=websecure"
      - "traefik.http.routers.pgadmin.tls=true"
      - "traefik.http.services.pgadmin.loadbalancer.server.port=80"

volumes:
  stackvo-pgadmin-data:
