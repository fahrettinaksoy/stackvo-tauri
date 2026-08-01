###################################################################
# STACKVO MARIADB COMPOSE TEMPLATE
###################################################################

services:
  mariadb:
    profiles: ["services", "mariadb"]  # --services for all, --profile mariadb for this service only
    image: "mariadb:{{ SERVICE_MARIADB_VERSION }}"
    container_name: "stackvo-mariadb"
    restart: unless-stopped

    environment:
      MARIADB_ROOT_PASSWORD: "{{ SERVICE_MARIADB_ROOT_PASSWORD | default('root') }}"
      MARIADB_DATABASE: "{{ SERVICE_MARIADB_DATABASE | default('stackvo') }}"

    volumes:
      - stackvo-mariadb-data:/var/lib/mysql
      - ${HOST_STACKVO_ROOT}/generated/configs/mariadb.cnf:/etc/mysql/conf.d/stackvo.cnf:ro
      # Log volume mount removed - logs go to stdout/stderr (GEMINI.md compliance)

    command: >
      mariadbd
      --character-set-server=utf8mb4
      --collation-server=utf8mb4_unicode_ci

    ports:
      - "{{ HOST_PORT_MARIADB | default('3307') }}:3306"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-mariadb-data:
