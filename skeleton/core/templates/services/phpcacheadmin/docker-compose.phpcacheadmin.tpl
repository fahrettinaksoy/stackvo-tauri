###################################################################
# STACKVO PHPCACHEADMIN COMPOSE TEMPLATE
###################################################################

services:
  phpcacheadmin:
    profiles: ["services", "phpcacheadmin"]  # --services for all, --profile phpcacheadmin for this service only
    image: "robinn/phpcacheadmin:{{ SERVICE_PHPCACHEADMIN_VERSION }}"
    container_name: "stackvo-phpcacheadmin"
    restart: unless-stopped

    environment:
      # Redis server configuration (PCA_ prefix required)
      PCA_REDIS_0_HOST: "{{ SERVICE_PHPCACHEADMIN_REDIS_HOST | default('stackvo-redis') }}"
      PCA_REDIS_0_PORT: "{{ SERVICE_PHPCACHEADMIN_REDIS_PORT | default('6379') }}"
      PCA_REDIS_0_CLIENT: "predis"  # Use Predis library instead of PHP Redis extension
      
      # Memcached server configuration (PCA_ prefix required)
      PCA_MEMCACHED_0_HOST: "{{ SERVICE_PHPCACHEADMIN_MEMCACHED_HOST | default('stackvo-memcached') }}"
      PCA_MEMCACHED_0_PORT: "{{ SERVICE_PHPCACHEADMIN_MEMCACHED_PORT | default('11211') }}"
      
      # Admin authentication
      PCA_ADMIN_USER: "{{ SERVICE_PHPCACHEADMIN_ADMIN_USER | default('admin') }}"
      PCA_ADMIN_PASS: "{{ SERVICE_PHPCACHEADMIN_ADMIN_PASS | default('admin') }}"
      
      # Disable metrics to avoid SQLite issues
      PCA_METRICS: "false"

    volumes:
      - stackvo-phpcacheadmin-data:/var/www/html/data
      - ${HOST_STACKVO_ROOT}/logs/services/phpcacheadmin:/var/log/phpcacheadmin

    ports:
      - "{{ SERVICE_PHPCACHEADMIN_HOST_PORT | default('8084') }}:80"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.phpcacheadmin.rule=Host(`{{ SERVICE_PHPCACHEADMIN_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.phpcacheadmin.entrypoints=websecure"
      - "traefik.http.routers.phpcacheadmin.tls=true"
      - "traefik.http.services.phpcacheadmin.loadbalancer.server.port=80"

volumes:
  stackvo-phpcacheadmin-data:
