###################################################################
# STACKVO POSTGRES COMPOSE TEMPLATE
###################################################################

services:
  postgres:
    profiles: ["services", "postgres"]  # --services for all, --profile postgres for this service only
    image: "postgres:{{ SERVICE_POSTGRES_VERSION }}"
    container_name: "stackvo-postgres"
    restart: unless-stopped

    environment:
      POSTGRES_DB: "{{ SERVICE_POSTGRES_DB | default('stackvo') }}"
      POSTGRES_USER: "{{ SERVICE_POSTGRES_USER | default('stackvo') }}"
      POSTGRES_PASSWORD: "{{ SERVICE_POSTGRES_PASSWORD | default('stackvo') }}"
      PGDATA: "/var/lib/postgresql/data/pgdata"

    volumes:
      - stackvo-postgres-data:/var/lib/postgresql/data/pgdata
      - ${HOST_STACKVO_ROOT}/logs/services/postgres:/var/log/postgresql

    ports:
      - "{{ SERVICE_POSTGRES_HOST_PORT | default('5432') }}:5432"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-postgres-data:
