#!/bin/bash
# Forge Restore Script
# Restores a backup created by backup.sh.
#
# Usage: ./restore.sh BACKUP_DIR
#
# Environment variables:
#   FORGED_POSTGRES_URL  - PostgreSQL connection URL (default: postgresql://forged:forged@localhost/forged)
#   FORGED_JJ_REPOS_ROOT - Jujutsu repos root (default: ./data/jj-repos)
#
# WARNING: This will DROP the existing database and replace it with the backup.

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 BACKUP_DIR"
    echo "  Restores a Forge backup from the specified directory."
    echo ""
    echo "WARNING: This will DROP the existing database."
    exit 1
fi

BACKUP_DIR="$1"
POSTGRES_URL="${FORGED_POSTGRES_URL:-postgresql://forged:forged@localhost/forged}"
JJ_REPOS_ROOT="${FORGED_JJ_REPOS_ROOT:-./data/jj-repos}"

if [ ! -d "${BACKUP_DIR}" ]; then
    echo "ERROR: Backup directory does not exist: ${BACKUP_DIR}"
    exit 1
fi

echo "=== Forge Restore ==="
echo "Restoring from: ${BACKUP_DIR}"

if [ -f "${BACKUP_DIR}/backup-metadata.json" ]; then
    echo "Backup metadata:"
    cat "${BACKUP_DIR}/backup-metadata.json"
    echo ""
fi

echo ""
echo "WARNING: This will DROP and recreate the database."
read -p "Continue? [y/N] " confirm
if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ]; then
    echo "Aborted."
    exit 0
fi

# 1. Restore PostgreSQL
if [ -f "${BACKUP_DIR}/forged.pgdump" ]; then
    echo "--- Restoring PostgreSQL ---"
    # Extract database name from URL
    DB_NAME=$(echo "${POSTGRES_URL}" | sed 's|.*/||')
    BASE_URL=$(echo "${POSTGRES_URL}" | sed "s|/${DB_NAME}$||")

    psql "${BASE_URL}/postgres" -c "DROP DATABASE IF EXISTS ${DB_NAME};"
    psql "${BASE_URL}/postgres" -c "CREATE DATABASE ${DB_NAME};"
    pg_restore --dbname="${POSTGRES_URL}" --no-owner --no-acl "${BACKUP_DIR}/forged.pgdump"
    echo "PostgreSQL restored."
else
    echo "WARNING: No PostgreSQL dump found in backup."
fi

# 2. Restore Jujutsu repositories
if [ -f "${BACKUP_DIR}/jj-repos.tar.gz" ]; then
    echo "--- Restoring Jujutsu repositories ---"
    PARENT_DIR=$(dirname "${JJ_REPOS_ROOT}")
    mkdir -p "${PARENT_DIR}"
    tar xzf "${BACKUP_DIR}/jj-repos.tar.gz" -C "${PARENT_DIR}"
    echo "JJ repos restored to ${JJ_REPOS_ROOT}"
else
    echo "WARNING: No JJ repos backup found."
fi

echo ""
echo "=== Restore complete ==="
echo "Run 'cargo run -p forged-migration' to ensure migrations are up to date."
