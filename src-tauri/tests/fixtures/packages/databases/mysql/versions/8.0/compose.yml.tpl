image: "{{ image }}"
container_name: "{{ instance.container }}"
restart: unless-stopped

environment:
  MYSQL_ROOT_PASSWORD: "{{ settings.ROOT_PASSWORD }}"
  MYSQL_DATABASE: "{{ settings.DATABASE }}"

volumes:
  - "{{ volume.data }}:/var/lib/mysql"
  - "{{ file.mysql_cnf }}:/etc/mysql/conf.d/stackvo.cnf:ro"
  - "{{ instance.logs }}:/var/log/mysql"

command: >
  mysqld
  --character-set-server=utf8mb4
  --collation-server=utf8mb4_unicode_ci
  --skip-character-set-client-handshake

ports:
  - "{{ port.main }}:3306"

networks:
  {{ network }}:
    aliases: {{ instance.aliases }}
