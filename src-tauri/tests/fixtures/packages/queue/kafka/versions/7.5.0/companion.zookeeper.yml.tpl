image: "{{ companion.image }}"
container_name: "{{ companion.instance.container }}"
restart: unless-stopped
environment:
  ZOOKEEPER_CLIENT_PORT: 2181
  ZOOKEEPER_TICK_TIME: 2000
volumes:
  - "{{ companion.instance.logs }}:/var/log/zookeeper"
ports:
  - "{{ companion.port.main }}:2181"
networks:
  {{ network }}:
    aliases: {{ companion.instance.aliases }}
