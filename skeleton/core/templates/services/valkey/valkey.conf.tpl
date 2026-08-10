###################################################################
# STACKVO VALKEY CONFIG TEMPLATE
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

# Security. Deliberately no placeholder here: this file is rendered into
# generated/configs/, and a password interpolated into a commented-out line
# is a password on disk in a file nobody thinks of as holding one.
# requirepass <your password>

# Enable notifications
notify-keyspace-events Ex
