###################################################################
# STACKVO MEILISEARCH COMPOSE TEMPLATE
# MEILI_ENV is pinned to development rather than exposed as a key:
# it is what keeps the bundled search preview reachable, and the
# production value additionally refuses to start on a master key
# shorter than 16 bytes — a failure that arrives as a container
# that exits, which is the least readable way to learn about a
# setting nobody chose.
###################################################################

services:
  meilisearch:
    profiles: ["services", "meilisearch"]  # --services for all, --profile meilisearch for this service only
    image: "getmeili/meilisearch:{{ SERVICE_MEILISEARCH_VERSION }}"
    container_name: "stackvo-meilisearch"
    restart: unless-stopped

    environment:
      MEILI_MASTER_KEY: "{{ SERVICE_MEILISEARCH_MASTER_KEY }}"
      MEILI_ENV: "development"
      MEILI_DB_PATH: /meili_data
      # Meilisearch reports usage to its vendor unless told otherwise.
      MEILI_NO_ANALYTICS: "true"

    volumes:
      - stackvo-meilisearch-data:/meili_data

    ports:
      - "{{ SERVICE_MEILISEARCH_HOST_PORT | default('7700') }}:7700"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.meilisearch.rule=Host(`{{ SERVICE_MEILISEARCH_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.meilisearch.entrypoints=websecure"
      - "traefik.http.routers.meilisearch.tls=true"
      - "traefik.http.services.meilisearch.loadbalancer.server.port=7700"

volumes:
  stackvo-meilisearch-data:
