###################################################################
# STACKVO MINIO COMPOSE TEMPLATE
# S3-compatible object storage. Two ports on purpose: 9000 is the
# S3 API an SDK talks to, 9001 is the browser console. The Traefik
# label routes the console, because the API is addressed by an
# endpoint URL and a client that already has one gains nothing
# from a second name for it.
###################################################################

services:
  minio:
    profiles: ["services", "minio"]  # --services for all, --profile minio for this service only
    image: "minio/minio:{{ SERVICE_MINIO_VERSION }}"
    container_name: "stackvo-minio"
    restart: unless-stopped

    command: server /data --console-address ":9001"

    environment:
      MINIO_ROOT_USER: "{{ SERVICE_MINIO_ROOT_USER }}"
      MINIO_ROOT_PASSWORD: "{{ SERVICE_MINIO_ROOT_PASSWORD }}"
      MINIO_REGION: "{{ SERVICE_MINIO_REGION | default('us-east-1') }}"
      # Off by default for the reason PRIVACY.md gives for the app itself.
      MINIO_UPDATE: "off"

    volumes:
      - stackvo-minio-data:/data

    ports:
      - "{{ SERVICE_MINIO_HOST_PORT | default('9000') }}:9000"
      - "{{ SERVICE_MINIO_CONSOLE_HOST_PORT | default('9001') }}:9001"

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.minio.rule=Host(`{{ SERVICE_MINIO_URL }}.{{ DEFAULT_TLD_SUFFIX }}`)"
      - "traefik.http.routers.minio.entrypoints=websecure"
      - "traefik.http.routers.minio.tls=true"
      - "traefik.http.services.minio.loadbalancer.server.port=9001"

volumes:
  stackvo-minio-data:
