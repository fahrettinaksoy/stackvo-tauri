image: "{{ image }}"
container_name: "{{ instance.container }}"
restart: unless-stopped

command: ["redis-server", "/etc/redis/redis.conf"]

volumes:
  - "{{ volume.data }}:/data"
  - "{{ file.redis_conf }}:/etc/redis/redis.conf:ro"
  - "{{ instance.logs }}:/var/log/redis"

ports:
  - "{{ port.main }}:6379"

networks:
  {{ network }}:
    aliases: {{ instance.aliases }}
