image: "{{ image }}"
container_name: "{{ instance.container }}"
restart: unless-stopped

environment:
  PMA_ARBITRARY: "{{ settings.ARBITRARY }}"
  PMA_HOST: "{{ settings.HOST }}"
  PMA_PORT: "{{ settings.PORT }}"
  UPLOAD_LIMIT: "{{ settings.UPLOAD_LIMIT }}"

volumes:
  - "{{ instance.logs }}:/var/log/phpmyadmin"

ports:
  - "{{ port.main }}:80"

networks:
  {{ network }}:
    aliases: {{ instance.aliases }}

labels:
  - "traefik.enable=true"
  - "traefik.http.routers.{{ instance.slug }}.rule=Host(`{{ instance.domain }}`)"
  - "traefik.http.routers.{{ instance.slug }}.entrypoints=websecure"
  - "traefik.http.routers.{{ instance.slug }}.tls=true"
  - "traefik.http.services.{{ instance.slug }}.loadbalancer.server.port=80"
