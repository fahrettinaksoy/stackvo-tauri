###################################################################
# STACKVO GRAFANA COMPOSE TEMPLATE
###################################################################

services:
  grafana:
    profiles: ["services", "grafana"]  # --services for all, --profile grafana for this service only
    image: "grafana/grafana:{{ SERVICE_GRAFANA_VERSION }}"
    container_name: "stackvo-grafana"
    restart: unless-stopped

    environment:
      GF_SECURITY_ADMIN_USER: "{{ SERVICE_GRAFANA_ADMIN_USER }}"
      GF_SECURITY_ADMIN_PASSWORD: "{{ SERVICE_GRAFANA_ADMIN_PASSWORD }}"
      GF_INSTALL_PLUGINS: ""
      GF_SERVER_ROOT_URL: "http://grafana.stackvo.{{ DEFAULT_TLD_SUFFIX | default('loc') }}"

    volumes:
      - stackvo-grafana-data:/var/lib/grafana
      - stackvo-grafana-config:/etc/grafana
      - ${HOST_STACKVO_ROOT}/logs/services/grafana:/var/log/grafana

    ports:
      - "{{ HOST_PORT_GRAFANA | default('3001') }}:3000"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    user: "{{ HOST_UID | default('472') }}"

volumes:
  stackvo-grafana-data:
  stackvo-grafana-config:
