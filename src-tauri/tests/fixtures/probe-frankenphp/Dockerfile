# syntax=docker/dockerfile:1.4

# Auto-generated Dockerfile for probe-frankenphp
# Web Server: FrankenPHP (Caddy + embedded PHP)
# PHP Version: 8.4
FROM dunglas/frankenphp:1-php8.4-bookworm

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
# Install PHP extensions via install-php-extensions
# FrankenPHP provides this tool (supports both standard and PECL extensions)
RUN install-php-extensions \
    mbstring \
    pdo \
    pdo_mysql \
    curl \
    zip \
    gd \
    intl \
    bcmath \
    redis \
    xdebug


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


# Copy FrankenPHP Caddyfile
COPY Caddyfile /etc/caddy/Caddyfile


WORKDIR /var/www/html

CMD ["frankenphp", "run", "--config", "/etc/caddy/Caddyfile"]
