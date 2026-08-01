###################################################################
# STACKVO MAILPIT COMPOSE TEMPLATE
# Maintained successor to MailHog: same SMTP/UI ports, so switching
# a project is a matter of which service is enabled — MailHog stays
# available for stacks that already run it.
###################################################################

services:
  mailpit:
    profiles: ["services", "mailpit"]  # --services for all, --profile mailpit for this service only
    image: "axllent/mailpit:{{ SERVICE_MAILPIT_VERSION }}"
    container_name: "stackvo-mailpit"
    restart: unless-stopped

    environment:
      MP_DATA_FILE: /data/mailpit.db

    ports:
      - "{{ HOST_PORT_MAILPIT_SMTP | default('1025') }}:1025"
      - "{{ HOST_PORT_MAILPIT_UI | default('8025') }}:8025"

    volumes:
      - stackvo-mailpit-data:/data

    networks:
      - "{{ DOCKER_DEFAULT_NETWORK }}"

volumes:
  stackvo-mailpit-data:
