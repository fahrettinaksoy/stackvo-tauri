###################################################################
# STACKVO MYSQL / MARIADB / PERCONA COMPOSE TEMPLATE
###################################################################

services:
  mysql:
    profiles: ["services", "mysql"]  # --services for all, --profile mysql for this service only
    image: "mysql:{{ SERVICE_MYSQL_VERSION }}"
    container_name: "stackvo-mysql"
    restart: unless-stopped

    environment:
      MYSQL_ROOT_PASSWORD: "{{ SERVICE_MYSQL_ROOT_PASSWORD | default('root') }}"
      MYSQL_DATABASE: "{{ SERVICE_MYSQL_DATABASE | default('stackvo') }}"

    volumes:
      - stackvo-mysql-data:/var/lib/mysql
      - ${HOST_STACKVO_ROOT}/generated/configs/mysql.cnf:/etc/mysql/conf.d/stackvo.cnf:ro
      - ${HOST_STACKVO_ROOT}/logs/services/mysql:/var/log/mysql

    command: >
      mysqld
      --character-set-server=utf8mb4
      --collation-server=utf8mb4_unicode_ci
      --skip-character-set-client-handshake

ports:
- "{{ HOST_PORT_MYSQL | default('3306') }}:3306"

networks:
- "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-mysql-data:
