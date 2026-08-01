###################################################################
# STACKVO REDIS CONFIG TEMPLATE
###################################################################

bind 0.0.0.0
port 6379

protected-mode no

# Memory optimizations
maxmemory 512mb
maxmemory-policy allkeys-lru

# Persistence
save 900 1
save 300 10
save 60 1000

appendonly yes
appendfsync everysec

# Performance
tcp-keepalive 300
timeout 0

# Logging
loglevel notice
logfile ""

# Security (optional, enable if needed)
# requirepass {{ REDIS_PASSWORD }}

# Enable notifications
notify-keyspace-events Ex
