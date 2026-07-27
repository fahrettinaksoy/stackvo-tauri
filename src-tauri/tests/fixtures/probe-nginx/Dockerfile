# syntax=docker/dockerfile:1.4

# Auto-generated Dockerfile for probe-nginx
# Web Server: Nginx + PHP-FPM
# PHP Version: 8.4
FROM php:8.4-fpm

# Install Nginx and Supervisord with BuildKit cache
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y \
    nginx \
    supervisor \
    && rm -rf /var/lib/apt/lists/*

# Install System Packages with BuildKit cache
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y \
    libcurl4-openssl-dev \
    libfreetype6-dev \
    libicu-dev \
    libjpeg-dev \
    libonig-dev \
    libpng-dev \
    libzip-dev \
    && rm -rf /var/lib/apt/lists/*

RUN docker-php-ext-configure gd --with-freetype --with-jpeg

# Install PHP extensions
RUN docker-php-ext-install pdo
RUN docker-php-ext-install pdo_mysql
RUN docker-php-ext-install mbstring curl zip gd intl bcmath

# Install PECL extensions
# Note: Versions are specified for stability, errors are tolerated

RUN --mount=type=cache,target=/tmp/pear,sharing=locked \
    pecl install igbinary-3.2.16 \
    && docker-php-ext-enable igbinary

RUN --mount=type=cache,target=/tmp/pear,sharing=locked \
    pecl install redis-6.3.0 \
    && docker-php-ext-enable redis

RUN --mount=type=cache,target=/tmp/pear,sharing=locked \
    pecl install xdebug-3.4.0 \
    && docker-php-ext-enable xdebug


# Install System Packages
RUN apt-get update && apt-get install -y \
    git \
    wget \
    unzip \
    default-mysql-client \
    postgresql-client \
    redis-tools \
    strace \
    vim \
    nano \
    curl \
    iputils-ping \
    net-tools \
    telnet \
    htop \
    procps \
    tar \
    gzip \
    bzip2 \
    p7zip-full \
    && rm -rf /var/lib/apt/lists/*


# Install Development Tools
# Install Composer
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

# Install Node.js 20.x
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*


# Configure PHP-FPM to listen on TCP port 127.0.0.1:9000
RUN sed -i 's|listen = .*|listen = 127.0.0.1:9000|' /usr/local/etc/php-fpm.d/www.conf

# Remove 'main' log format reference from Nginx config
RUN sed -i 's/ main;/;/' /etc/nginx/nginx.conf

# Disable default Nginx site (it conflicts with our config)
RUN rm -f /etc/nginx/sites-enabled/default

# Copy Nginx configuration
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Copy Supervisord configuration
COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf

# Create entrypoint script to ensure log directories exist at runtime
RUN echo '#!/bin/bash' > /entrypoint.sh && \
    echo 'mkdir -p /var/log/nginx /var/log/php-fpm' >> /entrypoint.sh && \
    echo 'touch /var/log/nginx/access.log /var/log/nginx/error.log' >> /entrypoint.sh && \
    echo 'chmod 666 /var/log/nginx/*.log' >> /entrypoint.sh && \
    echo 'exec "$@"' >> /entrypoint.sh && \
    chmod +x /entrypoint.sh

WORKDIR /var/www/html

# Use entrypoint to create log directories before starting supervisord
ENTRYPOINT ["/entrypoint.sh"]
CMD ["/usr/bin/supervisord", "-c", "/etc/supervisor/conf.d/supervisord.conf"]
