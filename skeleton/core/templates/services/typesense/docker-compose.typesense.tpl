###################################################################
# STACKVO TYPESENSE COMPOSE TEMPLATE
# Configured through TYPESENSE_* environment variables rather than
# server flags: the image reads both, and a flag list in `command`
# would put half the settings out of reach of the .env file the
# Services pane edits.
###################################################################

services:
  typesense:
    profiles: ["services", "typesense"]  # --services for all, --profile typesense for this service only
    image: "typesense/typesense:{{ SERVICE_TYPESENSE_VERSION }}"
    container_name: "stackvo-typesense"
    restart: unless-stopped

    environment:
      TYPESENSE_API_KEY: "{{ SERVICE_TYPESENSE_API_KEY }}"
      TYPESENSE_DATA_DIR: /data
      # A browser front end queries Typesense directly, from a different
      # origin than the one it was served from.
      TYPESENSE_ENABLE_CORS: "true"

    volumes:
      - stackvo-typesense-data:/data

    ports:
      - "{{ SERVICE_TYPESENSE_HOST_PORT | default('8108') }}:8108"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.typesense.rule=Host(`{{ SERVICE_TYPESENSE_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.typesense.entrypoints=websecure"
      - "traefik.http.routers.typesense.tls=true"
      - "traefik.http.services.typesense.loadbalancer.server.port=8108"

volumes:
  stackvo-typesense-data:
