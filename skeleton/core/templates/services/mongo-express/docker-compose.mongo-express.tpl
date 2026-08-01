###################################################################
# STACKVO MONGO EXPRESS COMPOSE TEMPLATE
###################################################################

services:
  mongo-express:
    profiles: ["services", "mongo-express"]  # --services for all, --profile mongo-express for this service only
    image: "mongo-express:{{ SERVICE_MONGO_EXPRESS_VERSION }}"
    container_name: "stackvo-mongo-express"
    restart: unless-stopped

    environment:
      ME_CONFIG_MONGODB_SERVER: "{{ SERVICE_MONGO_EXPRESS_MONGODB_SERVER | default('stackvo-mongo') }}"
      ME_CONFIG_MONGODB_PORT: "{{ SERVICE_MONGO_EXPRESS_MONGODB_PORT | default('27017') }}"
      ME_CONFIG_MONGODB_ADMINUSERNAME: "{{ SERVICE_MONGO_EXPRESS_ADMIN_USERNAME | default('root') }}"
      ME_CONFIG_MONGODB_ADMINPASSWORD: "{{ SERVICE_MONGO_EXPRESS_ADMIN_PASSWORD | default('root') }}"
      ME_CONFIG_BASICAUTH_USERNAME: "{{ SERVICE_MONGO_EXPRESS_BASICAUTH_USERNAME | default('admin') }}"
      ME_CONFIG_BASICAUTH_PASSWORD: "{{ SERVICE_MONGO_EXPRESS_BASICAUTH_PASSWORD | default('admin') }}"
      ME_CONFIG_SITE_BASEURL: "{{ SERVICE_MONGO_EXPRESS_BASEURL | default('/') }}"

    volumes:
      - ${HOST_STACKVO_ROOT}/logs/services/mongo-express:/var/log/mongo-express

    ports:
      - "{{ SERVICE_MONGO_EXPRESS_HOST_PORT | default('8081') }}:8081"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.mongo-express.rule=Host(`{{ SERVICE_MONGO_EXPRESS_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.mongo-express.entrypoints=websecure"
      - "traefik.http.routers.mongo-express.tls=true"
      - "traefik.http.services.mongo-express.loadbalancer.server.port=8081"
