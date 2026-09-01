# 43 - Backup & Recovery

## 💾 Backup and Disaster Recovery

Stackhouse's backup system (`stackhouse/src/storage/backups.rs`) produces **logical SQL dumps**, not filesystem/WAL snapshots — there's no embedded storage engine here to snapshot; the database is Postgres.

### Routes

Mounted under `/v1/admin`, require a service-admin JWT:

```
POST   /v1/admin/backups            # Create a backup: {"name": "..."}
GET    /v1/admin/backups            # List backups
POST   /v1/admin/backups/:id/restore  # Restore from a backup
DELETE /v1/admin/backups/:id        # Delete a backup
```

```bash
curl -X POST http://localhost:3000/v1/admin/backups \
  -H "Authorization: Bearer <service_admin_jwt>" \
  -H "Content-Type: application/json" \
  -d '{"name": "nightly"}'
```

### How a Backup Is Built

`BackupService::create_backup` walks `information_schema` for every user table (tables prefixed `stackhouse_` or `pg_` are skipped — **this means internal auth/session/billing tables are excluded from these backups**, only your own application tables are dumped) and writes a hand-generated `.sql` file (`CREATE TABLE IF NOT EXISTS ...` + row data, wrapped in `BEGIN;`/`COMMIT;`) to the server's local `backup_path`. Metadata (id, name, size, status) is tracked in the `stackhouse_backups` table. This is not a `pg_dump` wrapper — it's a custom logical-dump implementation, so exotic column types or constraints not reflected in `information_schema.columns` may not round-trip perfectly.

### Recovery Procedure

```bash
# List available backups
curl http://localhost:3000/v1/admin/backups -H "Authorization: Bearer <service_admin_jwt>"

# Restore a specific backup by id
curl -X POST http://localhost:3000/v1/admin/backups/<backup_id>/restore \
  -H "Authorization: Bearer <service_admin_jwt>"
```

Because the backup excludes `stackhouse_*` tables, a restore brings back your application data but not users/sessions/billing state — plan around that if you rely on this for full disaster recovery.

### Point-in-Time Recovery (PITR)

A separate `PitrService` (`stackhouse/src/storage/backups/pitr.rs`) implements point-in-time restore. It creates a restore schema, finds the nearest base backup before the target time, clones that base backup into the restore schema, and replays logical WAL entries from the `stackhouse_pitr_slot` up to the target timestamp using `PgOutputDecoder`.

**HTTP endpoint:**

```bash
POST /v1/admin/backups/pitr/restore
Authorization: Bearer <service-admin-jwt>
Content-Type: application/json

{
  "target_time": "2026-08-18T12:00:00Z"
}
```

**Response (on success):**

```json
{
  "success": true,
  "data": "<restore-operation-id>"
}
```

**Current limitations:**
- Restore requires a prior base backup (created by `PitrService` itself or a configured base-backup process) and a working logical replication slot.
- The restore produces a new isolated schema (`stackhouse_restore_<op>`) rather than overwriting the live database in place.
- The endpoint requires service-admin privileges, enforced through the same authorization path as the other `/v1/admin/backups/*` routes.

For self-hosted deployments that need native PostgreSQL PITR, you can also use PostgreSQL's own WAL archiving + `pg_basebackup`/`recovery_target_time` mechanism, or a managed Postgres provider's point-in-time restore feature (e.g. Cloud SQL, RDS) alongside this backup system.

---

**Next:** [API Reference](./50-API-Reference.md)
