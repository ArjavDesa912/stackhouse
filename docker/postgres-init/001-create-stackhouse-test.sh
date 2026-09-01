#!/bin/sh
set -eu

if ! psql -U "$POSTGRES_USER" -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = 'stackhouse_test'" | grep -q 1; then
  psql -U "$POSTGRES_USER" -d postgres -c "CREATE DATABASE stackhouse_test"
fi
