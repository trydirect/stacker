FROM postgres:18.3

RUN apt-get update \
    && apt-get install -y --no-install-recommends postgresql-18-cron \
    && rm -rf /var/lib/apt/lists/*

# pg_cron requires shared_preload_libraries and cron.database_name
# Configure these in postgresql.conf or pass -c flags at runtime.
