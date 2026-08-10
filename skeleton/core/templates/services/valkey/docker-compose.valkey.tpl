###################################################################
# STACKVO VALKEY COMPOSE TEMPLATE
# Redis's fork after the 2024 licence change, and wire-compatible
# with it — which is why the host port defaults to 6381 and not
# 6379: the two are meant to be runnable side by side while a
# project moves from one to the other, and a shared port makes
# whichever started second fail to bind.
###################################################################

services:
  valkey:
    profiles: ["services", "valkey"]  # --services for all, --profile valkey for this service only
    image: "valkey/valkey:{{ SERVICE_VALKEY_VERSION }}"
    container_name: "stackvo-valkey"
    restart: unless-stopped

    command: ["valkey-server", "/etc/valkey/valkey.conf"]

    volumes:
      - stackvo-valkey-data:/data
      - ${HOST_STACKVO_ROOT}/generated/configs/valkey.conf:/etc/valkey/valkey.conf:ro

    ports:
      - "{{ SERVICE_VALKEY_HOST_PORT | default('6381') }}:6379"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-valkey-data:
