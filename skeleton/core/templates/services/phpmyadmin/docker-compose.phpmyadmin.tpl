###################################################################
# STACKVO PHPMYADMIN COMPOSE TEMPLATE
###################################################################

services:
  phpmyadmin:
    profiles: ["services", "phpmyadmin"]  # --services for all, --profile phpmyadmin for this service only
    image: "phpmyadmin:{{ SERVICE_PHPMYADMIN_VERSION }}"
    container_name: "stackvo-phpmyadmin"
    restart: unless-stopped

    environment:
      PMA_ARBITRARY: "{{ SERVICE_PHPMYADMIN_ARBITRARY | default('1') }}"
      PMA_HOST: "{{ SERVICE_PHPMYADMIN_HOST | default('stackvo-mysql') }}"
      PMA_PORT: "{{ SERVICE_PHPMYADMIN_PORT | default('3306') }}"
      UPLOAD_LIMIT: "{{ SERVICE_PHPMYADMIN_UPLOAD_LIMIT | default('300M') }}"

    volumes:
      - ${HOST_STACKVO_ROOT}/logs/services/phpmyadmin:/var/log/phpmyadmin

    ports:
      - "{{ SERVICE_PHPMYADMIN_HOST_PORT | default('8081') }}:80"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.phpmyadmin.rule=Host(`{{ SERVICE_PHPMYADMIN_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.phpmyadmin.entrypoints=websecure"
      - "traefik.http.routers.phpmyadmin.tls=true"
      - "traefik.http.services.phpmyadmin.loadbalancer.server.port=80"
