###################################################################
# STACKVO MEMCACHED COMPOSE TEMPLATE
###################################################################

services:
  memcached:
    profiles: ["services", "memcached"]  # --services for all, --profile memcached for this service only
    image: "memcached:{{ SERVICE_MEMCACHED_VERSION }}"
    container_name: "stackvo-memcached"
    restart: unless-stopped

    command: >
      memcached
      -m {{ SERVICE_MEMCACHED_MEMORY | default('256') }}
      -c {{ SERVICE_MEMCACHED_CONNECTIONS | default('1024') }}
      -t {{ SERVICE_MEMCACHED_THREADS | default('4') }}
      {{ SERVICE_MEMCACHED_EXTRA_ARGS | default('') }}

    ports:
      - "{{ HOST_PORT_MEMCACHED | default('11211') }}:11211"

    volumes:
      - ${HOST_STACKVO_ROOT}/logs/services/memcached:/var/log/memcached

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"
