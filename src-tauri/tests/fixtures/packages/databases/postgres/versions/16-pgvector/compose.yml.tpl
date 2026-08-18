image: "{{ image }}"
container_name: "{{ instance.container }}"
restart: unless-stopped

environment:
  POSTGRES_PASSWORD: "{{ settings.PASSWORD }}"
  POSTGRES_DB: "{{ settings.DATABASE }}"

volumes:
  - "{{ volume.data }}:/var/lib/postgresql/data"

ports:
  - "{{ port.main }}:5432"

networks:
  {{ network }}:
    aliases: {{ instance.aliases }}
