###################################################################
# STACKVO MONGO COMPOSE TEMPLATE
###################################################################

services:
  mongo:
    profiles: ["services", "mongo"]  # --services for all, --profile mongo for this service only
    image: "mongo:{{ SERVICE_MONGO_VERSION }}"
    container_name: "stackvo-mongo"
    restart: unless-stopped

    environment:
      MONGO_INITDB_ROOT_USERNAME: "{{ SERVICE_MONGO_INITDB_ROOT_USERNAME | default('root') }}"
      MONGO_INITDB_ROOT_PASSWORD: "{{ SERVICE_MONGO_INITDB_ROOT_PASSWORD | default('root') }}"
      MONGO_INITDB_DATABASE: "{{ SERVICE_MONGO_DATABASE | default('stackvo') }}"

    volumes:
      - stackvo-mongo-data:/data/db
      - ${HOST_STACKVO_ROOT}/generated/configs/mongo.conf:/etc/mongo/mongo.conf:ro
      - ${HOST_STACKVO_ROOT}/logs/services/mongo:/var/log/mongodb

    ports:
      - "{{ HOST_PORT_MONGO | default('27017') }}:27017"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-mongo-data:
