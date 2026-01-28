init-db:
    psql -d ingest_dev -f db/create.sql

ingest-test:
    psql -d ingest_test -f db/create.sql
    cargo test
