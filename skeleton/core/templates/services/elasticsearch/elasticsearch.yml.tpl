###################################################################
# STACKVO ELASTICSEARCH SETTINGS TEMPLATE
###################################################################

cluster.name: "stackvo-es"
node.name: "stackvo-es-node-1"

path.data: /usr/share/elasticsearch/data

network.host: 0.0.0.0
http.port: 9200

# Disable xpack features unless enabled in .env
xpack.security.enabled: "{{ ELASTIC_SECURITY | default('false') }}"
xpack.security.transport.ssl.enabled: "{{ ELASTIC_SSL | default('false') }}"

# Improve performance for dev/local
indices.memory.index_buffer_size: 20%
thread_pool.write.queue_size: 1000
