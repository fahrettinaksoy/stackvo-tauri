###################################################################
# STACKVO MYSQL DEFAULT CONFIG (my.cnf)
###################################################################

[mysqld]
user=mysql
pid-file=/var/run/mysqld/mysqld.pid
socket=/var/run/mysqld/mysqld.sock

# Performance
innodb_buffer_pool_size=512M
innodb_log_file_size=256M
innodb_flush_method=O_DIRECT
innodb_flush_log_at_trx_commit=1
max_connections=200
thread_cache_size=50

# Character Set
character-set-server=utf8mb4
collation-server=utf8mb4_unicode_ci
init-connect='SET NAMES utf8mb4'
skip-character-set-client-handshake

# Logging
slow_query_log=1
long_query_time=2
slow_query_log_file=/var/log/mysql/slow.log

# General logs (disabled by default)
general_log=0
general_log_file=/var/log/mysql/general.log

[client]
default-character-set=utf8mb4

[mysql]
default-character-set=utf8mb4
