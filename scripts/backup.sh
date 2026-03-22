#!/bin/bash
# Forge Backup Script
# Backs up PostgreSQL database, SeaweedFS metadata, and Jujutsu repositories.
#
# Usage: ./backup.sh [BACKUP_DIR]
#
# Environment variables:
#   FORGED_POSTGRES_URL  - PostgreSQL connection URL (default: postgresql://forged:forged@localhost/forged)
#   FORGED_SEAWEEDFS_URL - SeaweedFS master URL (default: http://localhost:9333)
#   FORGED_JJ_REPOS_ROOT - Jujutsu repos root (default: ./data/jj-repos)

set -euo pipefail

BACKUP_DIR="${1:-./backups/$(date +%Y%m%d-%H%M%S)}"
POSTGRES_URL="${FORGED_POSTGRES_URL:-postgresql://forged:forged@localhost/forged}"
SEAWEEDFS_URL="${FORGED_SEAWEEDFS_URL:-http://localhost:9333}"
JJ_REPOS_ROOT="${FORGED_JJ_REPOS_ROOT:-./data/jj-repos}"

echo "=== Forge Backup ==="
echo "Backup directory: ${BACKUP_DIR}"
mkdir -p "${BACKUP_DIR}"

# 1. PostgreSQL dump
echo "--- Backing up PostgreSQL ---"
pg_dump "${POSTGRES_URL}" --format=custom --file="${BACKUP_DIR}/forged.pgdump"
echo "PostgreSQL backup: ${BACKUP_DIR}/forged.pgdump"

# 2. SeaweedFS volume list (metadata snapshot)
echo "--- Backing up SeaweedFS metadata ---"
curl -sf "${SEAWEEDFS_URL}/dir/status" > "${BACKUP_DIR}/seaweedfs-status.json" || echo "WARNING: Could not reach SeaweedFS"
echo "SeaweedFS status: ${BACKUP_DIR}/seaweedfs-status.json"

# 3. Jujutsu repositories
if [ -d "${JJ_REPOS_ROOT}" ]; then
    echo "--- Backing up Jujutsu repositories ---"
    tar czf "${BACKUP_DIR}/jj-repos.tar.gz" -C "$(dirname "${JJ_REPOS_ROOT}")" "$(basename "${JJ_REPOS_ROOT}")"
    echo "JJ repos backup: ${BACKUP_DIR}/jj-repos.tar.gz"
else
    echo "WARNING: Jujutsu repos directory not found at ${JJ_REPOS_ROOT}"
fi

# 4. Record backup metadata
cat > "${BACKUP_DIR}/backup-metadata.json" <<EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "postgres_url": "${POSTGRES_URL}",
  "seaweedfs_url": "${SEAWEEDFS_URL}",
  "jj_repos_root": "${JJ_REPOS_ROOT}",
  "hostname": "$(hostname)"
}
EOF

echo ""
echo "=== Backup complete ==="
echo "Total size: $(du -sh "${BACKUP_DIR}" | cut -f1)"
